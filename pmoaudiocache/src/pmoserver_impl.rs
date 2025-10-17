//! Implémentation du trait AudioCacheExt pour pmoserver::Server

#[cfg(feature = "pmoserver")]
use crate::{AudioCacheExt, Cache};
#[cfg(feature = "pmoserver")]
use pmocache::pmoserver_ext::{create_file_router, create_api_router};
#[cfg(feature = "pmoserver")]
use std::sync::Arc;
#[cfg(feature = "pmoserver")]
use utoipa::OpenApi;

#[cfg(feature = "pmoserver")]
impl AudioCacheExt for pmoserver::Server {
    async fn init_audio_cache(&mut self, cache_dir: &str, limit: usize) -> anyhow::Result<Arc<Cache>> {
        let base_url = self.info().base_url;
        let cache = Arc::new(crate::cache::new_cache(cache_dir, limit, &base_url)?);

        // Router de fichiers pour servir les pistes FLAC
        // Routes: GET /audio/tracks/{pk} et GET /audio/tracks/{pk}/{param}
        let file_router = create_file_router(
            cache.clone(),
            "audio/flac"  // Content-Type
        );
        self.add_router("/", file_router).await;

        // API REST générique (pmocache)
        // Routes: GET/POST/DELETE /api/audio, etc.
        let api_router = create_api_router(cache.clone());
        let openapi = crate::ApiDoc::openapi();
        self.add_openapi(api_router, openapi, "audio").await;

        Ok(cache)
    }

    async fn init_audio_cache_configured(&mut self) -> anyhow::Result<Arc<Cache>> {
        let config = pmoconfig::get_config();
        let cache_dir = config.get_audio_cache_dir()?;
        let limit = config.get_audio_cache_size()?;
        self.init_audio_cache(&cache_dir, limit).await
    }
}
