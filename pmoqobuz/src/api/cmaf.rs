//! Endpoints API CMAF de Qobuz : session/start et file/url.
//!
//! Ces endpoints implémentent le nouveau pipeline de streaming Qobuz
//! qui remplace progressivement `/track/getFileUrl`.

use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::info;

use crate::cmaf::crypto::{compute_request_sig, CMAF_SEED};
use crate::error::{QobuzError, Result};

/// État d'une session CMAF active.
pub struct CmafSession {
    pub session_id: String,
    pub infos: String,
    pub expires_at: u64,
}

/// Réponse de l'endpoint /session/start.
#[derive(Debug, Deserialize)]
struct SessionStartResponse {
    session_id: String,
    #[serde(default)]
    infos: Option<String>,
    expires_at: u64,
}

/// Réponse de l'endpoint /file/url (CMAF).
#[derive(Debug, Deserialize)]
pub struct CmafFileUrlResponse {
    /// Modèle d'URL avec le placeholder `$SEGMENT$`.
    #[serde(default)]
    pub url_template: Option<String>,
    /// Clé de contenu enveloppée, format `"qbz-1.wrapped_b64url.iv_b64url"`.
    #[serde(default)]
    pub key: Option<String>,
    /// Nombre de segments audio (hors segment init).
    pub n_segments: u8,
    #[serde(default)]
    pub format_id: Option<u32>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub sampling_rate: Option<u32>,
    /// Profondeur de bits (champ v1 de l'API).
    #[serde(default)]
    pub bits_depth: Option<u32>,
    /// Profondeur de bits (champ v2 de l'API).
    #[serde(default)]
    pub bit_depth: Option<u32>,
}

impl CmafFileUrlResponse {
    /// Retourne la profondeur de bits en préférant `bits_depth` puis `bit_depth`.
    pub fn resolved_bit_depth(&self) -> Option<u32> {
        self.bits_depth.or(self.bit_depth)
    }
}

const BASE_URL: &str = "https://www.qobuz.com/api.json/0.2";

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Signe une requête /session/start.
fn sign_session_start(timestamp: u64) -> String {
    let mut args = std::collections::BTreeMap::new();
    args.insert("profile", "qbz-1".to_string());
    compute_request_sig("sessionstart", &args, &timestamp.to_string(), CMAF_SEED)
}

/// Signe une requête /file/url.
fn sign_file_url(track_id: &str, format_id: u32, timestamp: u64) -> String {
    let mut args = std::collections::BTreeMap::new();
    args.insert("format_id", format_id.to_string());
    args.insert("intent", "stream".to_string());
    args.insert("track_id", track_id.to_string());
    compute_request_sig("fileurl", &args, &timestamp.to_string(), CMAF_SEED)
}

/// Gestionnaire de session CMAF avec renouvellement automatique.
///
/// Utilise le pattern double-checked lock : fast path sous read guard,
/// slow path sous write guard exclusif pour éviter les renouvellements
/// concurrents qui produiraient des `infos` incohérents avec les `key`
/// retournées par `/file/url`.
pub struct CmafSessionManager {
    session: RwLock<Option<CmafSession>>,
}

impl CmafSessionManager {
    pub fn new() -> Self {
        Self { session: RwLock::new(None) }
    }

    /// Retourne `(session_id, infos)` d'une session valide, en en démarrant
    /// une nouvelle si la session courante est absente ou expire dans < 60s.
    pub async fn ensure_session(
        &self,
        http: &reqwest::Client,
        app_id: &str,
        auth_token: &str,
    ) -> Result<(String, String)> {
        let now = current_timestamp();

        // Fast path : session existante avec > 60s restants.
        {
            let guard = self.session.read().await;
            if let Some(ref cs) = *guard {
                if cs.expires_at > now + 60 {
                    return Ok((cs.session_id.clone(), cs.infos.clone()));
                }
            }
        }

        // Slow path : prend le verrou d'écriture et vérifie à nouveau.
        let mut guard = self.session.write().await;
        if let Some(ref cs) = *guard {
            if cs.expires_at > now + 60 {
                return Ok((cs.session_id.clone(), cs.infos.clone()));
            }
        }

        info!("[CMAF] Démarrage d'une nouvelle session");
        let timestamp = current_timestamp();
        let sig = sign_session_start(timestamp);

        let url = format!("{}/session/start", BASE_URL);
        let response = http
            .post(&url)
            .header("X-App-Id", app_id)
            .header("X-User-Auth-Token", auth_token)
            .form(&[
                ("profile", "qbz-1"),
                ("request_ts", &timestamp.to_string()),
                ("request_sig", &sig),
            ])
            .send()
            .await
            .map_err(QobuzError::Http)?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(QobuzError::ApiError {
                code: status.as_u16(),
                message: format!("session/start échoué: {}", body),
            });
        }

        let resp: SessionStartResponse = response.json().await.map_err(|e| {
            QobuzError::Other(format!("parse session/start: {}", e))
        })?;

        let infos = resp.infos.unwrap_or_default();
        info!(
            "[CMAF] Session démarrée: id={}..., expires_at={}",
            &resp.session_id[..resp.session_id.len().min(8)],
            resp.expires_at,
        );

        let session_id = resp.session_id.clone();
        let infos_clone = infos.clone();

        *guard = Some(CmafSession {
            session_id: resp.session_id,
            infos,
            expires_at: resp.expires_at,
        });

        Ok((session_id, infos_clone))
    }

    /// Invalide la session courante (utile en cas d'erreur de déchiffrement).
    pub async fn invalidate(&self) {
        *self.session.write().await = None;
    }
}

/// Récupère l'URL de fichier CMAF pour un track.
///
/// Inclut le retry sur les erreurs transitoires (5xx, 429, erreurs réseau).
/// Un 404 retourne immédiatement une erreur `NotFound`.
pub async fn get_file_url(
    http: &reqwest::Client,
    app_id: &str,
    auth_token: &str,
    session_id: &str,
    track_id: &str,
    format_id: u32,
) -> Result<CmafFileUrlResponse> {
    use crate::retry::{classify_reqwest, classify_status, retry_transient, DEFAULT_MAX_ATTEMPTS};

    let url = format!("{}/file/url", BASE_URL);

    let result = retry_transient(
        DEFAULT_MAX_ATTEMPTS,
        "CMAF file/url",
        |e: &QobuzError| matches!(e, QobuzError::RateLimitExceeded | QobuzError::ApiError { code: 500..=599, .. }),
        |_attempt| {
            let url = url.clone();
            async move {
                let timestamp = current_timestamp();
                let sig = sign_file_url(track_id, format_id, timestamp);

                let response = http
                    .get(&url)
                    .header("X-App-Id", app_id)
                    .header("X-User-Auth-Token", auth_token)
                    .header("X-Session-Id", session_id)
                    .query(&[
                        ("track_id", track_id),
                        ("format_id", &format_id.to_string()),
                        ("intent", "stream"),
                        ("request_ts", &timestamp.to_string()),
                        ("request_sig", &sig),
                    ])
                    .send()
                    .await
                    .map_err(QobuzError::Http)?;

                let status = response.status();
                tracing::info!("[CMAF] file/url track_id={} format_id={} status={}", track_id, format_id, status);

                if !status.is_success() {
                    let code = status.as_u16();
                    return Err(match code {
                        404 => QobuzError::NotFound(format!("track {} non disponible", track_id)),
                        429 => QobuzError::RateLimitExceeded,
                        _ => QobuzError::ApiError {
                            code,
                            message: format!("file/url status {}", code),
                        },
                    });
                }

                let file_url: CmafFileUrlResponse = response.json().await.map_err(|e| {
                    QobuzError::Other(format!("parse file/url: {}", e))
                })?;

                tracing::info!(
                    "[CMAF] file/url: n_segments={}, mime={:?}, sampling_rate={:?}",
                    file_url.n_segments,
                    file_url.mime_type,
                    file_url.sampling_rate,
                );

                Ok(file_url)
            }
        },
    )
    .await?;

    Ok(result)
}
