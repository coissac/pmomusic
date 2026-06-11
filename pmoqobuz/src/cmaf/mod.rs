//! Pipeline CMAF (Common Media Application Format) pour Qobuz.
//!
//! Qobuz utilise CMAF avec chiffrement AES-CTR par frame sur CDN Akamai.
//! C'est le pipeline de l'app Android v9.7+ qui remplace l'endpoint legacy
//! `/track/getFileUrl`.
//!
//! # Pipeline
//!
//! 1. `/session/start` → `{ session_id, infos, expires_at }`
//! 2. `/file/url` → `{ url_template, key (enveloppé), n_segments, ... }`
//! 3. Session key = `HKDF(CMAF_SEED, infos)`
//! 4. Content key = AES-CBC-unwrap(session_key, key)
//! 5. Segment init (s=0) → header FLAC + table des segments
//! 6. Pour chaque s=1..n_segments : fetch → parse crypto boxes → déchiffrement AES-CTR

pub mod crypto;
pub mod error;
pub mod parser;

pub use crypto::{compute_request_sig, decrypt_frame, derive_session_key, unwrap_content_key, CMAF_SEED};
pub use error::CmafError;
pub use parser::{
    parse_init_segment, parse_segment_crypto, FrameEntry, InitInfo, SegmentCrypto,
    SegmentTableEntry,
};

use std::sync::Arc;
use tokio::io::AsyncRead;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

use crate::error::{QobuzError, Result};
use crate::retry::{classify_reqwest, classify_status, retry_transient, FetchError, DEFAULT_MAX_ATTEMPTS};

/// Concurrence max pour le fetch de segments CMAF.
/// 3 segments en vol est le compromis optimal — le CDN Akamai rate-limite
/// au-delà de ~5 requêtes parallèles par IP sur des fenêtres de 1s.
pub const CMAF_PREFETCH_CONCURRENCY: usize = 3;

/// Callback de progression pour les fonctions de téléchargement.
pub type CmafProgressCallback = Arc<dyn Fn(CmafProgressUpdate) + Send + Sync>;

/// Un tick de progression. `segments_completed` est cumulatif (1..=n).
#[derive(Debug, Clone, Copy)]
pub struct CmafProgressUpdate {
    pub segments_completed: u32,
    pub n_segments: u32,
    pub bytes_this_segment: u64,
}

/// Info réunies depuis le segment init, suffisantes pour démarrer le streaming.
pub struct CmafStreamingInfo {
    pub url_template: String,
    pub n_segments: u8,
    pub content_key: [u8; 16],
    pub flac_header: Vec<u8>,
    pub segment_table: Vec<SegmentTableEntry>,
    pub format_id: u32,
    pub sampling_rate: Option<u32>,
    pub bit_depth: Option<u32>,
}

/// Construit un client reqwest dédié aux fetches CDN Akamai.
fn build_cdn_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| QobuzError::Http(e))
}

/// Fetch une URL CDN en bytes avec retry sur les erreurs transitoires.
/// Un 404/403 échoue immédiatement (terminal). 5xx et 429 → retry avec backoff.
async fn fetch_bytes_with_retry(
    http: &reqwest::Client,
    url: &str,
    log_tag: &str,
) -> std::result::Result<Vec<u8>, FetchError> {
    retry_transient(
        DEFAULT_MAX_ATTEMPTS,
        log_tag,
        FetchError::is_transient,
        |_attempt| async move {
            let response = http
                .get(url)
                .header("User-Agent", "Mozilla/5.0")
                .send()
                .await
                .map_err(|e| classify_reqwest(&e, "fetch"))?;
            let status = response.status();
            if !status.is_success() {
                return Err(classify_status(status, "fetch"));
            }
            response
                .bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| classify_reqwest(&e, "lecture"))
        },
    )
    .await
}

/// Fetch les segments 1..=n_segments avec contrôle de concurrence.
/// Déclenche le callback de progression une fois par segment complété.
/// Les résultats sont retournés triés par index de segment.
async fn fetch_all_segments(
    http: &reqwest::Client,
    url_template: &str,
    n_segments: u8,
    log_tag: &str,
    on_progress: Option<CmafProgressCallback>,
) -> Result<Vec<Vec<u8>>> {
    let semaphore = Arc::new(Semaphore::new(CMAF_PREFETCH_CONCURRENCY));
    let completed_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let mut handles = Vec::with_capacity(n_segments as usize);

    for seg_idx in 1u8..=n_segments {
        let sem = semaphore.clone();
        let http = http.clone();
        let seg_url = url_template.replace("$SEGMENT$", &seg_idx.to_string());
        let log_tag = log_tag.to_string();
        let progress = on_progress.clone();
        let counter = completed_count.clone();

        handles.push(tokio::spawn(async move {
            let permit = sem.acquire_owned().await
                .map_err(|e| format!("semaphore: {}", e))?;

            let seg_data = fetch_bytes_with_retry(&http, &seg_url, &format!("{} seg {}", log_tag, seg_idx))
                .await
                .map_err(|e| format!("[{}] seg {} fetch: {}", log_tag, seg_idx, e))?;

            let bytes_this_segment = seg_data.len() as u64;
            if let Some(cb) = progress {
                let done = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                cb(CmafProgressUpdate {
                    segments_completed: done,
                    n_segments: n_segments as u32,
                    bytes_this_segment,
                });
            }

            // Pause avant de libérer le slot pour respecter les limites CDN
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            drop(permit);

            Ok::<(u8, Vec<u8>), String>((seg_idx, seg_data))
        }));
    }

    let mut segments: Vec<(u8, Vec<u8>)> = Vec::with_capacity(handles.len());
    for handle in handles {
        let (idx, data) = handle
            .await
            .map_err(|e| QobuzError::Other(format!("[{}] panic task: {}", log_tag, e)))?
            .map_err(|e| QobuzError::Other(format!("[{}] téléchargement échoué: {}", log_tag, e)))?;
        segments.push((idx, data));
    }
    segments.sort_by_key(|(idx, _)| *idx);
    Ok(segments.into_iter().map(|(_, data)| data).collect())
}

/// Déchiffre un segment CMAF et ajoute les frames FLAC dans `output`.
///
/// Optimisation : extend + decrypt in-place, zéro allocation par frame.
fn decrypt_one_segment(
    seg_data: &[u8],
    content_key: &[u8; 16],
    output: &mut Vec<u8>,
    seg_idx: usize,
) -> Result<()> {
    let crypto = parse_segment_crypto(seg_data)
        .map_err(|e| QobuzError::Other(format!("CMAF seg {} parse: {}", seg_idx, e)))?;

    let mut data_pos = crypto.data_offset;
    for entry in &crypto.entries {
        let frame_end = data_pos + entry.size as usize;
        if frame_end > seg_data.len() {
            return Err(QobuzError::Other(format!("CMAF seg {} débordement frame", seg_idx)));
        }
        let start = output.len();
        output.extend_from_slice(&seg_data[data_pos..frame_end]);
        if entry.flags != 0 {
            decrypt_frame(content_key, &entry.iv, &mut output[start..]);
        }
        data_pos = frame_end;
    }
    if data_pos < crypto.mdat_end && crypto.mdat_end <= seg_data.len() {
        output.extend_from_slice(&seg_data[data_pos..crypto.mdat_end]);
    }
    Ok(())
}

/// Déchiffre une séquence de segments CMAF chiffrés et écrit les frames FLAC dans `output`.
pub fn decrypt_segments_into(
    segments: &[Vec<u8>],
    content_key: &[u8; 16],
    output: &mut Vec<u8>,
) -> Result<()> {
    for (seg_idx, seg_data) in segments.iter().enumerate() {
        decrypt_one_segment(seg_data, content_key, output, seg_idx + 1)?;
    }
    Ok(())
}

/// Prépare le streaming CMAF : dérive les clés, fetche le segment init.
/// Ne télécharge PAS les segments audio — l'appelant les streame en arrière-plan.
pub async fn setup_streaming(
    url_template: String,
    key_str: &str,
    infos: &str,
    n_segments: u8,
    format_id: u32,
    sampling_rate: Option<u32>,
    bit_depth: Option<u32>,
) -> Result<CmafStreamingInfo> {
    let session_key = derive_session_key(CMAF_SEED, infos)
        .map_err(|e| QobuzError::Other(format!("dérivation clé session: {}", e)))?;
    let content_key = unwrap_content_key(&session_key, key_str)
        .map_err(|e| QobuzError::Other(format!("dérobage clé contenu: {}", e)))?;

    let http = build_cdn_client()?;
    let init_url = url_template.replace("$SEGMENT$", "0");

    info!("[CMAF] Fetch segment init: {}", &init_url[..init_url.len().min(60)]);

    let init_data = fetch_bytes_with_retry(&http, &init_url, "CMAF init")
        .await
        .map_err(|e| QobuzError::Other(format!("fetch segment init: {}", e)))?;

    let init_info = parse_init_segment(&init_data)
        .map_err(|e| QobuzError::Other(format!("parse segment init: {}", e)))?;

    info!(
        "[CMAF] Init: header FLAC {}B, {} segments dans la table, n_segments API={}",
        init_info.flac_header.len(),
        init_info.segment_table.len(),
        n_segments,
    );
    if init_info.segment_table.len() != n_segments as usize {
        warn!(
            "[CMAF] ÉCART: table={} entrées mais API dit n_segments={}",
            init_info.segment_table.len(),
            n_segments,
        );
    }

    Ok(CmafStreamingInfo {
        url_template,
        n_segments,
        content_key,
        flac_header: init_info.flac_header,
        segment_table: init_info.segment_table,
        format_id,
        sampling_rate,
        bit_depth,
    })
}

/// Ouvre un flux FLAC déchiffré en mode progressif.
///
/// Retourne un `AsyncRead` qui produit les bytes FLAC au fur et à mesure que les
/// segments CMAF sont téléchargés et déchiffrés. Le lecteur peut commencer à
/// consommer le flux (et le cache à le servir) avant que tous les segments
/// soient téléchargés — le progressive caching est préservé.
///
/// Le deuxième élément est la taille totale estimée en bytes, calculée depuis
/// le header FLAC et la table des segments du segment init.
///
/// # Erreurs
///
/// Retourne une erreur si la dérivation des clés ou le fetch du segment init
/// échoue. Les erreurs de segments ultérieurs provoquent la fermeture du pipe
/// (le lecteur verra un EOF prématuré).
pub async fn open_flac_stream(
    url_template: String,
    key_str: String,
    infos: String,
    n_segments: u8,
    format_id: u32,
    sampling_rate: Option<u32>,
    bit_depth: Option<u32>,
) -> Result<(impl AsyncRead + Send + Unpin + 'static, u64)> {
    use tokio::io::AsyncWriteExt;

    let setup = setup_streaming(
        url_template, &key_str, &infos, n_segments,
        format_id, sampling_rate, bit_depth,
    ).await?;

    let estimated_size = (setup.flac_header.len()
        + setup.segment_table.iter().map(|s| s.byte_len as usize).sum::<usize>()) as u64;

    // Pipe 256 KB : assez grand pour absorber un segment FLAC Hi-Res typique
    // sans bloquer le producteur, assez petit pour ne pas sur-allouer.
    let (mut writer, reader) = tokio::io::duplex(256 * 1024);

    tokio::spawn(async move {
        if let Err(e) = writer.write_all(&setup.flac_header).await {
            warn!("[CMAF-STREAM] écriture header FLAC: {}", e);
            return;
        }

        let http = match build_cdn_client() {
            Ok(c) => c,
            Err(e) => { warn!("[CMAF-STREAM] client HTTP: {}", e); return; }
        };

        // Lancer tous les fetches avec concurrence bornée par le sémaphore.
        // Les handles sont stockés dans l'ordre — on les consomme en ordre.
        let sem = Arc::new(Semaphore::new(CMAF_PREFETCH_CONCURRENCY));
        let mut handles = Vec::with_capacity(setup.n_segments as usize);

        for seg_idx in 1u8..=setup.n_segments {
            let sem = sem.clone();
            let http = http.clone();
            let url = setup.url_template.replace("$SEGMENT$", &seg_idx.to_string());

            handles.push(tokio::spawn(async move {
                let permit = sem.acquire_owned().await
                    .map_err(|e| format!("sémaphore: {}", e))?;
                let result = fetch_bytes_with_retry(&http, &url, &format!("CMAF-STREAM seg {}", seg_idx))
                    .await
                    .map_err(|e| format!("seg {} fetch: {}", seg_idx, e));
                // Cooldown CDN Akamai : retenir le slot 500ms avant de libérer,
                // identique à fetch_all_segments, pour éviter le rate-limiting.
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                drop(permit);
                result
            }));
        }

        // Consommer dans l'ordre : segment 1 d'abord, puis 2, etc.
        // Les segments prêts en avance attendent dans leur JoinHandle.
        let mut buf = Vec::new();
        for (i, handle) in handles.into_iter().enumerate() {
            let seg_idx = i + 1;
            let seg_data = match handle.await {
                Ok(Ok(data)) => data,
                Ok(Err(e)) => { warn!("[CMAF-STREAM] seg {}: {}", seg_idx, e); return; }
                Err(e) => { warn!("[CMAF-STREAM] seg {} panique: {}", seg_idx, e); return; }
            };

            buf.clear();
            if let Err(e) = decrypt_one_segment(&seg_data, &setup.content_key, &mut buf, seg_idx) {
                warn!("[CMAF-STREAM] seg {}: déchiffrement: {}", seg_idx, e);
                return;
            }
            if let Err(e) = writer.write_all(&buf).await {
                // Le lecteur a fermé le pipe (ex: playback stoppé) — arrêt silencieux.
                debug!("[CMAF-STREAM] seg {}: pipe fermé ({})", seg_idx, e);
                return;
            }
            debug!("[CMAF-STREAM] seg {}/{} → {} B", seg_idx, setup.n_segments, buf.len());
        }

        info!("[CMAF-STREAM] complet : {} segments, {:.2} MB estimés",
            setup.n_segments,
            estimated_size as f64 / (1024.0 * 1024.0));
        // writer dropped ici → EOF propre sur le reader
    });

    Ok((reader, estimated_size))
}

/// Télécharge un track CMAF complet et retourne les bytes FLAC déchiffrés.
pub async fn download_full(
    url_template: String,
    key_str: &str,
    infos: &str,
    n_segments: u8,
    format_id: u32,
    sampling_rate: Option<u32>,
    bit_depth: Option<u32>,
    on_progress: Option<CmafProgressCallback>,
) -> Result<Vec<u8>> {
    let setup = setup_streaming(url_template, key_str, infos, n_segments, format_id, sampling_rate, bit_depth).await?;
    let http = build_cdn_client()?;

    let total_size: usize = setup.flac_header.len()
        + setup.segment_table.iter().map(|s| s.byte_len as usize).sum::<usize>();

    let segments = fetch_all_segments(&http, &setup.url_template, setup.n_segments, "CMAF-FULL", on_progress).await?;

    let mut output = Vec::with_capacity(total_size);
    output.extend_from_slice(&setup.flac_header);
    decrypt_segments_into(&segments, &setup.content_key, &mut output)?;

    debug!(
        "[CMAF-FULL] Complet: {:.2} MB FLAC, attendu {:.2} MB",
        output.len() as f64 / (1024.0 * 1024.0),
        total_size as f64 / (1024.0 * 1024.0),
    );
    Ok(output)
}
