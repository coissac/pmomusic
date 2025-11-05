//! Télécharge un bloc Radio Paradise, le cache, et le joue en même temps
//!
//! Ce programme démontre l'utilisation complète de la chaîne :
//! 1. RadioParadiseStreamSource - Télécharge et décode un bloc FLAC
//! 2. FlacCacheSink - Cache chaque piste en FLAC et alimente une playlist
//! 3. PlaylistSource - Lit la playlist pendant le téléchargement
//! 4. AudioSink - Joue l'audio sur la sortie standard
//!
//! Architecture :
//! ```text
//! Pipeline 1 (Download & Cache):
//!   RadioParadiseStreamSource → FlacCacheSink (avec playlist abonnée)
//!
//! Pipeline 2 (Playback):
//!   PlaylistSource (lit la playlist) → AudioSink (joue l'audio)
//! ```
//!
//! Usage:
//!   cargo run --example play_and_cache --features full -- <channel_id>
//!
//! Exemple:
//!   cargo run --example play_and_cache --features full -- 0    # Main Mix
//!   cargo run --example play_and_cache --features full -- 2    # Rock Mix

use pmoaudio::{AudioPipelineNode, AudioSink};
use pmoaudio_ext::{FlacCacheSink, PlaylistSource};
use pmoaudiocache::Cache as AudioCache;
use pmocovers::Cache as CoverCache;
use pmoparadise::{RadioParadiseClient, RadioParadiseStreamSource};
use std::env;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialiser tracing avec beaucoup de logs
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::DEBUG.into())
                .add_directive("pmoaudio=debug".parse()?)
                .add_directive("pmoaudio_ext=debug".parse()?)
                .add_directive("pmoplaylist=debug".parse()?)
                .add_directive("pmoparadise=debug".parse()?)
                .add_directive("pmoaudiocache=debug".parse()?)
        )
        .init();

    tracing::info!("=== Radio Paradise Play & Cache ===");

    // Récupérer les arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <channel_id>", args[0]);
        eprintln!();
        eprintln!("Downloads a Radio Paradise block, caches it, and plays it simultaneously.");
        eprintln!();
        eprintln!("Channel IDs:");
        eprintln!("  0 - Main Mix (eclectic, diverse mix)");
        eprintln!("  1 - Mellow Mix (smooth, chilled music)");
        eprintln!("  2 - Rock Mix (classic & modern rock)");
        eprintln!("  3 - World/Etc Mix (global sounds)");
        std::process::exit(1);
    }

    let channel_id: u8 = match args[1].parse() {
        Ok(id) if id <= 3 => id,
        _ => {
            eprintln!("Error: channel_id must be a number between 0 and 3");
            std::process::exit(1);
        }
    };

    tracing::info!("Channel ID: {}", channel_id);

    // ═══════════════════════════════════════════════════════════════════════════
    // Initialiser les caches et le gestionnaire de playlist
    // ═══════════════════════════════════════════════════════════════════════════

    let base_dir = std::env::var("PMO_CONFIG_DIR").unwrap_or_else(|_| "/tmp/pmomusic_test".to_string());
    std::fs::create_dir_all(&base_dir)?;

    tracing::info!("Initializing caches in: {}", base_dir);

    // Créer le cache audio
    let audio_cache_dir = format!("{}/audio_cache", base_dir);
    std::fs::create_dir_all(&audio_cache_dir)?;
    let audio_cache = Arc::new(AudioCache::new(
        &audio_cache_dir,
        1000, // 1000 MB limit
    )?);
    tracing::debug!("Audio cache initialized at: {}", audio_cache_dir);

    // Créer le cache de covers
    let cover_cache_dir = format!("{}/cover_cache", base_dir);
    std::fs::create_dir_all(&cover_cache_dir)?;
    let cover_cache = Arc::new(CoverCache::new(
        &cover_cache_dir,
        100, // 100 MB limit
    )?);
    tracing::debug!("Cover cache initialized at: {}", cover_cache_dir);

    // Enregistrer les caches dans le registre global pmoupnp
    // (requis par pmoplaylist pour valider les pks)
    pmoupnp::register_audio_cache(audio_cache.clone());
    pmoupnp::register_cover_cache(cover_cache.clone());
    tracing::debug!("Caches registered in pmoupnp global registry");

    // Utiliser le gestionnaire de playlist singleton
    tracing::info!("Getting playlist manager...");
    let playlist_manager = pmoplaylist::PlaylistManager();
    tracing::debug!("Playlist manager obtained");

    // ═══════════════════════════════════════════════════════════════════════════
    // Créer la playlist pour ce channel
    // ═══════════════════════════════════════════════════════════════════════════

    let playlist_id = format!("radio-paradise-ch{}", channel_id);
    tracing::info!("Creating playlist: {}", playlist_id);

    // Créer la playlist (ou la vider si elle existe)
    let mut writer = playlist_manager.create_persistent_playlist(playlist_id.clone()).await?;
    writer.set_title(format!("Radio Paradise - Channel {}", channel_id)).await?;
    writer.flush().await?; // Vider la playlist si elle existait
    tracing::debug!("Playlist created and flushed");

    // Créer le reader pour la lecture
    let reader = playlist_manager.get_read_handle(&playlist_id).await?;
    tracing::debug!("Read handle created");

    // ═══════════════════════════════════════════════════════════════════════════
    // Récupérer les infos du bloc à télécharger
    // ═══════════════════════════════════════════════════════════════════════════

    tracing::info!("Fetching current block metadata...");
    let client = RadioParadiseClient::builder()
        .channel(channel_id)
        .build()
        .await?;

    let block = client.get_block(None).await?;

    tracing::info!("Block Information:");
    tracing::info!("  Event ID: {}", block.event);
    tracing::info!("  Songs: {}", block.song_count());
    tracing::info!("  Duration: {:.1} minutes", block.length as f64 / 60000.0);
    tracing::info!("");

    tracing::info!("Tracklist:");
    for (index, song) in block.songs_ordered() {
        tracing::info!(
            "  {:2}. {} - {} ({})",
            index + 1,
            song.artist,
            song.title,
            song.album.as_deref().unwrap_or("Unknown Album")
        );
    }
    tracing::info!("");

    // ═══════════════════════════════════════════════════════════════════════════
    // Pipeline 1: Téléchargement et cache
    // ═══════════════════════════════════════════════════════════════════════════

    tracing::info!("Creating download pipeline...");

    // Créer la source Radio Paradise
    let mut download_source = RadioParadiseStreamSource::new(client);
    download_source.push_block_id(block.event);
    tracing::debug!("RadioParadiseStreamSource created with block {}", block.event);

    // Créer le sink de cache FLAC
    let mut cache_sink = FlacCacheSink::new(audio_cache.clone(), cover_cache.clone());
    cache_sink.register_playlist(writer);
    tracing::debug!("FlacCacheSink created and registered with playlist");

    // Connecter source → sink
    download_source.register(Box::new(cache_sink));
    tracing::info!("Download pipeline connected: RadioParadiseStreamSource → FlacCacheSink");

    // ═══════════════════════════════════════════════════════════════════════════
    // Pipeline 2: Lecture depuis la playlist
    // ═══════════════════════════════════════════════════════════════════════════

    tracing::info!("Creating playback pipeline...");

    // Créer la source playlist
    let mut playlist_source = PlaylistSource::new(reader, audio_cache.clone());
    tracing::debug!("PlaylistSource created");

    // Créer le sink audio
    let audio_sink = AudioSink::new();
    tracing::debug!("AudioSink created");

    // Connecter playlist → audio
    playlist_source.register(Box::new(audio_sink));
    tracing::info!("Playback pipeline connected: PlaylistSource → AudioSink");

    // ═══════════════════════════════════════════════════════════════════════════
    // Lancer les deux pipelines en parallèle
    // ═══════════════════════════════════════════════════════════════════════════

    tracing::info!("");
    tracing::info!("========================================");
    tracing::info!("Starting both pipelines...");
    tracing::info!("Pipeline 1: Downloading and caching");
    tracing::info!("Pipeline 2: Playing from playlist");
    tracing::info!("========================================");
    tracing::info!("");

    let stop_token = CancellationToken::new();
    let stop_token_download = stop_token.clone();
    let stop_token_playback = stop_token.clone();

    // Gérer Ctrl+C
    let stop_token_ctrl_c = stop_token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::warn!("Received Ctrl+C, stopping...");
        stop_token_ctrl_c.cancel();
    });

    let start = std::time::Instant::now();

    // Lancer les deux pipelines en parallèle
    let download_handle = tokio::spawn(async move {
        tracing::info!("[DOWNLOAD] Pipeline starting...");
        let result = Box::new(download_source).run(stop_token_download).await;
        match &result {
            Ok(()) => tracing::info!("[DOWNLOAD] Pipeline completed successfully"),
            Err(e) => tracing::error!("[DOWNLOAD] Pipeline error: {}", e),
        }
        result
    });

    let playback_handle = tokio::spawn(async move {
        // Attendre un peu que le premier track soit disponible
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        tracing::info!("[PLAYBACK] Pipeline starting...");
        let result = Box::new(playlist_source).run(stop_token_playback).await;
        match &result {
            Ok(()) => tracing::info!("[PLAYBACK] Pipeline completed successfully"),
            Err(e) => tracing::error!("[PLAYBACK] Pipeline error: {}", e),
        }
        result
    });

    // Attendre les deux pipelines
    let (download_result, playback_result) = tokio::join!(download_handle, playback_handle);

    let elapsed = start.elapsed();

    // Vérifier les résultats
    match (download_result, playback_result) {
        (Ok(Ok(())), Ok(Ok(()))) => {
            tracing::info!("");
            tracing::info!("========================================");
            tracing::info!("✓ Both pipelines completed successfully");
            tracing::info!("  Total time: {:.2}s", elapsed.as_secs_f64());
            tracing::info!("========================================");
        }
        (download_res, playback_res) => {
            tracing::error!("");
            tracing::error!("========================================");
            if let Err(e) = download_res {
                tracing::error!("✗ Download pipeline error: {:?}", e);
            } else if let Ok(Err(e)) = download_res {
                tracing::error!("✗ Download pipeline error: {}", e);
            }
            if let Err(e) = playback_res {
                tracing::error!("✗ Playback pipeline error: {:?}", e);
            } else if let Ok(Err(e)) = playback_res {
                tracing::error!("✗ Playback pipeline error: {}", e);
            }
            tracing::error!("========================================");
            return Err("Pipeline error".into());
        }
    }

    Ok(())
}
