//! API REST pour la gestion du cache de couvertures
//!
//! Ce module expose une API REST documentée avec OpenAPI/Swagger pour :
//! - Lister les images en cache
//! - Ajouter des images depuis une URL
//! - Supprimer des images
//! - Consulter les statistiques

use crate::{Cache, CacheEntry};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

/// Requête pour ajouter une image au cache
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddImageRequest {
    /// URL de l'image source
    #[schema(example = "https://example.com/cover.jpg")]
    pub url: String,
}

/// Réponse après ajout d'une image
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddImageResponse {
    /// Clé primaire (pk) de l'image ajoutée
    #[schema(example = "1a2b3c4d5e6f7a8b")]
    pub pk: String,
    /// URL source de l'image
    #[schema(example = "https://example.com/cover.jpg")]
    pub url: String,
    /// Message de succès
    #[schema(example = "Image added successfully")]
    pub message: String,
}

/// Réponse de suppression d'une image
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeleteImageResponse {
    /// Message de succès
    #[schema(example = "Image deleted successfully")]
    pub message: String,
}

/// Réponse d'erreur générique
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    /// Code d'erreur
    #[schema(example = "NOT_FOUND")]
    pub error: String,
    /// Message descriptif
    #[schema(example = "Image not found in cache")]
    pub message: String,
}

/// Liste toutes les images en cache avec leurs statistiques
///
/// Retourne la liste complète des entrées du cache triées par nombre d'accès décroissant.
#[utoipa::path(
    get,
    path = "/api/covers",
    responses(
        (status = 200, description = "Liste des images en cache", body = Vec<CacheEntry>),
        (status = 500, description = "Erreur serveur", body = ErrorResponse)
    ),
    tag = "covers"
)]
pub async fn list_images(State(cache): State<Arc<Cache>>) -> impl IntoResponse {
    match cache.db.get_all() {
        Ok(entries) => (StatusCode::OK, Json(entries)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Cannot retrieve cache entries: {}", e),
            }),
        )
            .into_response(),
    }
}

/// Récupère les informations d'une image spécifique
///
/// Retourne les métadonnées d'une image identifiée par sa clé (pk).
#[utoipa::path(
    get,
    path = "/api/covers/{pk}",
    params(
        ("pk" = String, Path, description = "Clé primaire de l'image", example = "1a2b3c4d5e6f7a8b")
    ),
    responses(
        (status = 200, description = "Informations de l'image", body = CacheEntry),
        (status = 404, description = "Image non trouvée", body = ErrorResponse)
    ),
    tag = "covers"
)]
pub async fn get_image_info(
    State(cache): State<Arc<Cache>>,
    Path(pk): Path<String>,
) -> impl IntoResponse {
    match cache.db.get(&pk) {
        Ok(entry) => (StatusCode::OK, Json(entry)).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NOT_FOUND".to_string(),
                message: format!("Image with pk '{}' not found in cache", pk),
            }),
        )
            .into_response(),
    }
}

/// Ajoute une image au cache depuis une URL
///
/// Télécharge l'image depuis l'URL fournie, la convertit en WebP et l'ajoute au cache.
/// Si l'image existe déjà, elle est mise à jour.
#[utoipa::path(
    post,
    path = "/api/covers",
    request_body = AddImageRequest,
    responses(
        (status = 201, description = "Image ajoutée avec succès", body = AddImageResponse),
        (status = 400, description = "Requête invalide", body = ErrorResponse),
        (status = 500, description = "Erreur lors du téléchargement ou de la conversion", body = ErrorResponse)
    ),
    tag = "covers"
)]
pub async fn add_image(
    State(cache): State<Arc<Cache>>,
    Json(req): Json<AddImageRequest>,
) -> impl IntoResponse {
    if req.url.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "URL cannot be empty".to_string(),
            }),
        )
            .into_response();
    }

    match cache.add_from_url(&req.url).await {
        Ok(pk) => (
            StatusCode::CREATED,
            Json(AddImageResponse {
                pk,
                url: req.url,
                message: "Image added successfully".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "PROCESSING_ERROR".to_string(),
                message: format!("Cannot add image: {}", e),
            }),
        )
            .into_response(),
    }
}

/// Supprime une image du cache
///
/// Supprime l'image et toutes ses variantes du disque et de la base de données.
#[utoipa::path(
    delete,
    path = "/api/covers/{pk}",
    params(
        ("pk" = String, Path, description = "Clé primaire de l'image à supprimer", example = "1a2b3c4d5e6f7a8b")
    ),
    responses(
        (status = 200, description = "Image supprimée avec succès", body = DeleteImageResponse),
        (status = 404, description = "Image non trouvée", body = ErrorResponse),
        (status = 500, description = "Erreur lors de la suppression", body = ErrorResponse)
    ),
    tag = "covers"
)]
pub async fn delete_image(
    State(cache): State<Arc<Cache>>,
    Path(pk): Path<String>,
) -> impl IntoResponse {
    // Vérifier que l'image existe
    if cache.db.get(&pk).is_err() {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NOT_FOUND".to_string(),
                message: format!("Image with pk '{}' not found in cache", pk),
            }),
        )
            .into_response();
    }

    // Supprimer les fichiers (original + variantes)
    let orig_path = cache.dir.join(format!("{}.orig.webp", pk));
    if orig_path.exists() {
        if let Err(e) = tokio::fs::remove_file(&orig_path).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "FILE_DELETE_ERROR".to_string(),
                    message: format!("Cannot delete original file: {}", e),
                }),
            )
                .into_response();
        }
    }

    // Supprimer toutes les variantes (*.{pk}.*.webp)
    if let Ok(mut entries) = tokio::fs::read_dir(&cache.dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Some(filename) = entry.file_name().to_str() {
                if filename.starts_with(&pk) && filename.ends_with(".webp") && filename != format!("{}.orig.webp", pk) {
                    let _ = tokio::fs::remove_file(entry.path()).await;
                }
            }
        }
    }

    // Supprimer de la base de données
    match cache.db.delete(&pk) {
        Ok(_) => (
            StatusCode::OK,
            Json(DeleteImageResponse {
                message: format!("Image '{}' deleted successfully", pk),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Cannot delete from database: {}", e),
            }),
        )
            .into_response(),
    }
}

/// Purge complètement le cache
///
/// Supprime toutes les images et vide la base de données. Opération irréversible.
#[utoipa::path(
    delete,
    path = "/api/covers",
    responses(
        (status = 200, description = "Cache purgé avec succès", body = DeleteImageResponse),
        (status = 500, description = "Erreur lors de la purge", body = ErrorResponse)
    ),
    tag = "covers"
)]
pub async fn purge_cache(State(cache): State<Arc<Cache>>) -> impl IntoResponse {
    match cache.purge().await {
        Ok(_) => (
            StatusCode::OK,
            Json(DeleteImageResponse {
                message: "Cache purged successfully".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "PURGE_ERROR".to_string(),
                message: format!("Cannot purge cache: {}", e),
            }),
        )
            .into_response(),
    }
}

/// Consolide le cache
///
/// Re-télécharge les images manquantes et supprime les fichiers orphelins.
/// Utile pour réparer un cache corrompu.
#[utoipa::path(
    post,
    path = "/api/covers/consolidate",
    responses(
        (status = 200, description = "Cache consolidé avec succès", body = DeleteImageResponse),
        (status = 500, description = "Erreur lors de la consolidation", body = ErrorResponse)
    ),
    tag = "covers"
)]
pub async fn consolidate_cache(State(cache): State<Arc<Cache>>) -> impl IntoResponse {
    match cache.consolidate().await {
        Ok(_) => (
            StatusCode::OK,
            Json(DeleteImageResponse {
                message: "Cache consolidated successfully".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "CONSOLIDATE_ERROR".to_string(),
                message: format!("Cannot consolidate cache: {}", e),
            }),
        )
            .into_response(),
    }
}
