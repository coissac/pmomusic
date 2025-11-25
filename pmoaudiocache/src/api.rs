//! API REST handlers spécifiques au cache audio

use crate::metadata_ext::AudioTrackMetadataExt;
use crate::Cache;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use pmometadata::TrackMetadata;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use utoipa::ToSchema;

/// Réponse contenant l'URL de la cover avec fallback
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CoverUrlResponse {
    /// PK de la piste
    #[schema(example = "1a2b3c4d5e6f7a8b")]
    pub pk: String,
    /// URL de la cover (cover_pk, cover_url, ou data URL par défaut)
    #[schema(example = "https://example.com/cover.jpg")]
    pub cover_url: String,
    /// Source de l'URL: "cover_pk", "cover_url", ou "default"
    #[schema(example = "cover_pk")]
    pub source: String,
}

/// Récupère l'URL de la cover d'une piste avec logique de fallback
///
/// Cette route retourne l'URL de la cover en appliquant la logique de priorité suivante :
/// 1. Si `cover_pk` est défini dans les métadonnées, retourne la clé du cache de covers
/// 2. Sinon, si `cover_url` est défini, retourne l'URL externe
/// 3. Sinon, retourne une image SVG par défaut (data URL)
///
/// # Arguments
///
/// * `pk` - Clé primaire de la piste audio
///
/// # Responses
///
/// * `200 OK` - Retourne l'URL de la cover avec la source
/// * `404 NOT_FOUND` - Piste non trouvée
/// * `500 INTERNAL_SERVER_ERROR` - Erreur lors de la lecture des métadonnées
#[utoipa::path(
    get,
    path = "/{pk}/cover-url",
    tag = "audio",
    params(
        ("pk" = String, Path, description = "Clé primaire de la piste")
    ),
    responses(
        (status = 200, description = "URL de la cover récupérée avec succès", body = CoverUrlResponse),
        (status = 404, description = "Piste non trouvée", body = pmocache::api::ErrorResponse),
        (status = 500, description = "Erreur interne", body = pmocache::api::ErrorResponse),
    )
)]
pub async fn get_cover_url(
    State(cache): State<Arc<Cache>>,
    Path(pk): Path<String>,
) -> impl IntoResponse {
    // Vérifier que la piste existe
    if cache.db.get(&pk, false).is_err() {
        return (
            StatusCode::NOT_FOUND,
            Json(pmocache::api::ErrorResponse {
                error: "NOT_FOUND".to_string(),
                message: format!("Track with pk '{}' not found in cache", pk),
            }),
        )
            .into_response();
    }

    // Récupérer les métadonnées
    let metadata = cache.track_metadata(&pk);
    let metadata_guard = metadata.read().await;

    // Déterminer la source et l'URL
    let (cover_url, source) = match metadata_guard.get_cover_pk().await {
        Ok(Some(cover_pk)) if !cover_pk.is_empty() => (cover_pk, "cover_pk".to_string()),
        _ => match metadata_guard.get_cover_url().await {
            Ok(Some(url)) if !url.is_empty() => (url, "cover_url".to_string()),
            _ => (
                pmometadata::get_default_cover_url(),
                "default".to_string(),
            ),
        },
    };

    (
        StatusCode::OK,
        Json(CoverUrlResponse {
            pk,
            cover_url,
            source,
        }),
    )
        .into_response()
}
