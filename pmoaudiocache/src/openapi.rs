//! Documentation OpenAPI pour l'API du cache audio

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "PMOMusic Audio Cache API",
        version = "0.1.0",
        description = "API de gestion du cache de pistes audio avec conversion FLAC asynchrone"
    ),
    components(
        schemas(
            crate::db::AudioCacheEntry,
            crate::metadata::AudioMetadata,
            crate::api::AddTrackRequest,
        )
    ),
    tags(
        (name = "audio", description = "Gestion des pistes audio")
    )
)]
pub struct ApiDoc;
