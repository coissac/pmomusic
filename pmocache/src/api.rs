//! API REST générique pour la gestion du cache
//!
//! Ce module expose une API REST documentée avec OpenAPI/Swagger pour :
//! - Lister les items en cache
//! - Ajouter des items depuis une URL
//! - Consulter le status des downloads en cours
//! - Supprimer des items
//! - Purger et consolider le cache

use crate::{Cache, CacheConfig};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Statut d'un téléchargement
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct DownloadStatus {
    /// Clé primaire de l'item
    #[cfg_attr(feature = "openapi", schema(example = "1a2b3c4d5e6f7a8b"))]
    pub pk: String,
    /// Téléchargement en cours
    pub in_progress: bool,
    /// Taille actuelle téléchargée (source)
    pub current_size: Option<u64>,
    /// Taille après transformation
    pub transformed_size: Option<u64>,
    /// Taille totale attendue
    pub expected_size: Option<u64>,
    /// Téléchargement terminé
    pub finished: bool,
    /// Erreur éventuelle
    pub error: Option<String>,
    /// Informations sur la conversion
    pub conversion: Option<ConversionStatus>,
}

/// Informations sur la conversion en cours ou réalisée
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ConversionStatus {
    /// Mode de conversion (ex: "passthrough", "transcode")
    #[cfg_attr(feature = "openapi", schema(example = "passthrough"))]
    pub mode: String,
    /// Codec source détecté (si disponible)
    pub input_codec: Option<String>,
    /// Informations complémentaires lisibles (optionnel)
    pub details: Option<String>,
}

/// Requête pour ajouter un item au cache.
///
/// Au moins une des deux entrées (`url` ou `path`) doit être fournie.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AddItemRequest {
    /// URL HTTP/HTTPS/UPnP à télécharger
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(example = "https://example.com/file.dat"))]
    pub url: Option<String>,
    /// Chemin local (`file://` implicite) à référencer
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(example = "/mnt/music/track.flac"))]
    pub path: Option<String>,
    /// Collection optionnelle
    #[cfg_attr(feature = "openapi", schema(example = "album:the_wall"))]
    pub collection: Option<String>,
}

/// Réponse après ajout d'un item
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AddItemResponse {
    /// Clé primaire (pk) de l'item ajouté
    #[cfg_attr(feature = "openapi", schema(example = "1a2b3c4d5e6f7a8b"))]
    pub pk: String,
    /// URL ou chemin source de l'item
    #[cfg_attr(feature = "openapi", schema(example = "https://example.com/file.dat"))]
    pub url: String,
    /// Message de succès
    #[cfg_attr(feature = "openapi", schema(example = "Item added successfully"))]
    pub message: String,
}

/// Réponse de suppression d'un item
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct DeleteItemResponse {
    /// Message de succès
    #[cfg_attr(feature = "openapi", schema(example = "Item deleted successfully"))]
    pub message: String,
}

/// Réponse d'erreur générique
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ErrorResponse {
    /// Code d'erreur
    #[cfg_attr(feature = "openapi", schema(example = "NOT_FOUND"))]
    pub error: String,
    /// Message descriptif
    #[cfg_attr(feature = "openapi", schema(example = "Item not found in cache"))]
    pub message: String,
}

/// Requête pour définir un TTL
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SetTtlRequest {
    /// Date/heure d'expiration au format RFC3339
    #[cfg_attr(feature = "openapi", schema(example = "2025-01-20T10:30:00Z"))]
    pub expires_at: String,
}

/// Réponse pour une opération de pinning/TTL
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PinResponse {
    /// Clé primaire de l'item
    #[cfg_attr(feature = "openapi", schema(example = "1a2b3c4d5e6f7a8b"))]
    pub pk: String,
    /// Message de succès
    #[cfg_attr(feature = "openapi", schema(example = "Item pinned successfully"))]
    pub message: String,
}

/// Statut de pinning d'un item
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PinStatus {
    /// Clé primaire de l'item
    #[cfg_attr(feature = "openapi", schema(example = "1a2b3c4d5e6f7a8b"))]
    pub pk: String,
    /// Indique si l'item est épinglé
    pub pinned: bool,
    /// Date/heure d'expiration du TTL (si défini)
    #[cfg_attr(feature = "openapi", schema(example = "2025-01-20T10:30:00Z"))]
    pub ttl_expires_at: Option<String>,
}

/// Liste tous les items en cache avec leurs statistiques
///
/// Retourne la liste complète des entrées du cache triées par nombre d'accès décroissant.
pub async fn list_items<C: CacheConfig + 'static>(
    State(cache): State<Arc<Cache<C>>>,
) -> impl IntoResponse {
    match cache.db.get_all(true) {
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

/// Récupère les informations d'un item spécifique
///
/// Retourne les métadonnées d'un item identifié par sa clé (pk).
pub async fn get_item_info<C: CacheConfig + 'static>(
    State(cache): State<Arc<Cache<C>>>,
    Path(pk): Path<String>,
) -> impl IntoResponse {
    match cache.db.get(&pk, true) {
        Ok(entry) => (StatusCode::OK, Json(entry)).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NOT_FOUND".to_string(),
                message: format!("Item with pk '{}' not found in cache", pk),
            }),
        )
            .into_response(),
    }
}

/// Récupère le statut du téléchargement d'un item
///
/// Retourne le statut actuel du téléchargement (progression, tailles, erreurs).
/// Si le téléchargement est terminé, retourne les informations du fichier.
pub async fn get_download_status<C: CacheConfig + 'static>(
    State(cache): State<Arc<Cache<C>>>,
    Path(pk): Path<String>,
) -> impl IntoResponse {
    let entry = match cache.db.get(&pk, false) {
        Ok(entry) => entry,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NOT_FOUND".to_string(),
                    message: format!("Item with pk '{}' not found in cache", pk),
                }),
            )
                .into_response();
        }
    };

    let download = cache.get_download(&pk).await;
    let file_path = cache.get_file_path(&pk);
    let file_size = if file_path.exists() {
        std::fs::metadata(&file_path).ok().map(|m| m.len())
    } else {
        None
    };

    let in_progress = download.is_some();
    let current_size = if let Some(download) = download.as_ref() {
        Some(download.current_size().await)
    } else {
        file_size
    };

    let transformed_size = if let Some(download) = download.as_ref() {
        Some(download.transformed_size().await)
    } else {
        file_size
    };

    let expected_size = if let Some(download) = download.as_ref() {
        download.expected_size().await
    } else {
        file_size
    };

    let finished = if let Some(download) = download.as_ref() {
        download.finished().await
    } else {
        file_path.exists()
    };

    let error = if let Some(download) = download.as_ref() {
        download.error().await
    } else {
        None
    };

    let mut conversion = if let Some(download) = download.as_ref() {
        download
            .transform_metadata()
            .await
            .map(ConversionStatus::from)
    } else {
        None
    };

    if conversion.is_none() {
        if let Some(meta) = entry.metadata.as_ref() {
            conversion = conversion_from_json(meta);
        }
    }

    let status = DownloadStatus {
        pk,
        in_progress,
        current_size,
        transformed_size,
        expected_size,
        finished,
        error,
        conversion,
    };

    (StatusCode::OK, Json(status)).into_response()
}

impl From<crate::download::TransformMetadata> for ConversionStatus {
    fn from(value: crate::download::TransformMetadata) -> Self {
        Self {
            mode: value.mode.unwrap_or_else(|| "unknown".to_string()),
            input_codec: value.input_codec,
            details: value.details,
        }
    }
}

fn conversion_from_json(value: &Value) -> Option<ConversionStatus> {
    value
        .get("conversion")
        .and_then(|conv| serde_json::from_value(conv.clone()).ok())
}

#[derive(Clone, Copy)]
enum AddSource<'a> {
    Url(&'a str),
    Local(&'a str),
}

/// Ajoute un item au cache depuis une URL
///
/// Télécharge l'item depuis l'URL fournie et l'ajoute au cache.
/// Si l'item existe déjà, il est mis à jour.
pub async fn add_item<C: CacheConfig + 'static>(
    State(cache): State<Arc<Cache<C>>>,
    Json(req): Json<AddItemRequest>,
) -> impl IntoResponse {
    let mode = match (req.url.as_deref(), req.path.as_deref()) {
        (Some(url), None) if !url.is_empty() => AddSource::Url(url),
        (None, Some(path)) if !path.is_empty() => AddSource::Local(path),
        (Some(_), Some(_)) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_REQUEST".to_string(),
                    message: "Provide either 'url' or 'path', not both".to_string(),
                }),
            )
                .into_response()
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_REQUEST".to_string(),
                    message: "Either 'url' or 'path' must be provided".to_string(),
                }),
            )
                .into_response()
        }
    };

    let collection = req.collection.as_deref();
    let add_result = match mode {
        AddSource::Url(url) => cache.add_from_url(url, collection).await,
        AddSource::Local(path) => cache.add_from_file(path, collection).await,
    };

    match add_result {
        Ok(pk) => {
            let origin =
                cache
                    .db
                    .get_origin_url(&pk)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| match mode {
                        AddSource::Url(url) => url.to_string(),
                        AddSource::Local(path) => format!("file://{}", path),
                    });

            (
                StatusCode::CREATED,
                Json(AddItemResponse {
                    pk,
                    url: origin,
                    message: "Item added successfully".to_string(),
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "PROCESSING_ERROR".to_string(),
                message: format!("Cannot add item: {}", e),
            }),
        )
            .into_response(),
    }
}

/// Supprime un item du cache
///
/// Supprime l'item et toutes ses variantes du disque et de la base de données.
pub async fn delete_item<C: CacheConfig + 'static>(
    State(cache): State<Arc<Cache<C>>>,
    Path(pk): Path<String>,
) -> impl IntoResponse {
    // Vérifier que l'item existe
    if cache.db.get(&pk, false).is_err() {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NOT_FOUND".to_string(),
                message: format!("Item with pk '{}' not found in cache", pk),
            }),
        )
            .into_response();
    }

    // Supprimer tous les fichiers avec ce pk (toutes variantes)
    let cache_dir = cache.cache_dir();
    if let Ok(mut entries) = tokio::fs::read_dir(cache_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Some(filename) = entry.file_name().to_str() {
                // Format: {pk}.{param}.{ext}
                if filename.starts_with(&pk) && filename.starts_with(&format!("{}.", pk)) {
                    let _ = tokio::fs::remove_file(entry.path()).await;
                }
            }
        }
    }

    // Supprimer de la base de données
    match cache.db.delete(&pk) {
        Ok(_) => (
            StatusCode::OK,
            Json(DeleteItemResponse {
                message: format!("Item '{}' deleted successfully", pk),
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
/// Supprime tous les items et vide la base de données. Opération irréversible.
pub async fn purge_cache<C: CacheConfig + 'static>(
    State(cache): State<Arc<Cache<C>>>,
) -> impl IntoResponse {
    match cache.purge().await {
        Ok(_) => (
            StatusCode::OK,
            Json(DeleteItemResponse {
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
/// Re-télécharge les items manquants et supprime les fichiers orphelins.
/// Utile pour réparer un cache corrompu.
pub async fn consolidate_cache<C: CacheConfig + 'static>(
    State(cache): State<Arc<Cache<C>>>,
) -> impl IntoResponse {
    match cache.consolidate().await {
        Ok(_) => (
            StatusCode::OK,
            Json(DeleteItemResponse {
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

/// Récupère le statut de pinning d'un item
///
/// Retourne si l'item est épinglé et sa date d'expiration TTL (si défini).
pub async fn get_pin_status<C: CacheConfig + 'static>(
    State(cache): State<Arc<Cache<C>>>,
    Path(pk): Path<String>,
) -> impl IntoResponse {
    // Vérifier que l'item existe et récupérer ses infos
    match cache.db.get(&pk, false) {
        Ok(entry) => (
            StatusCode::OK,
            Json(PinStatus {
                pk,
                pinned: entry.pinned,
                ttl_expires_at: entry.ttl_expires_at,
            }),
        )
            .into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NOT_FOUND".to_string(),
                message: format!("Item with pk '{}' not found in cache", pk),
            }),
        )
            .into_response(),
    }
}

/// Épingle un item pour le protéger de l'éviction LRU
///
/// Un item épinglé ne peut pas être supprimé automatiquement et ne compte pas
/// dans la limite du cache. Échoue si l'item a un TTL défini.
pub async fn pin_item<C: CacheConfig + 'static>(
    State(cache): State<Arc<Cache<C>>>,
    Path(pk): Path<String>,
) -> impl IntoResponse {
    match cache.pin(&pk).await {
        Ok(_) => (
            StatusCode::OK,
            Json(PinResponse {
                pk: pk.clone(),
                message: format!("Item '{}' pinned successfully", pk),
            }),
        )
            .into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("TTL") {
                (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        error: "CONFLICT".to_string(),
                        message: "Cannot pin an item with TTL set. Clear TTL first.".to_string(),
                    }),
                )
                    .into_response()
            } else if error_msg.contains("no rows") {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: "NOT_FOUND".to_string(),
                        message: format!("Item with pk '{}' not found in cache", pk),
                    }),
                )
                    .into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "PIN_ERROR".to_string(),
                        message: format!("Cannot pin item: {}", e),
                    }),
                )
                    .into_response()
            }
        }
    }
}

/// Désépingle un item
///
/// Rend l'item à nouveau éligible à l'éviction LRU et le compte dans la limite du cache.
pub async fn unpin_item<C: CacheConfig + 'static>(
    State(cache): State<Arc<Cache<C>>>,
    Path(pk): Path<String>,
) -> impl IntoResponse {
    match cache.unpin(&pk).await {
        Ok(_) => (
            StatusCode::OK,
            Json(PinResponse {
                pk: pk.clone(),
                message: format!("Item '{}' unpinned successfully", pk),
            }),
        )
            .into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("no rows") {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: "NOT_FOUND".to_string(),
                        message: format!("Item with pk '{}' not found in cache", pk),
                    }),
                )
                    .into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "UNPIN_ERROR".to_string(),
                        message: format!("Cannot unpin item: {}", e),
                    }),
                )
                    .into_response()
            }
        }
    }
}

/// Définit le TTL (Time To Live) d'un item
///
/// L'item sera automatiquement supprimé à la date d'expiration.
/// Échoue si l'item est épinglé.
pub async fn set_item_ttl<C: CacheConfig + 'static>(
    State(cache): State<Arc<Cache<C>>>,
    Path(pk): Path<String>,
    Json(req): Json<SetTtlRequest>,
) -> impl IntoResponse {
    // Valider le format de la date
    if chrono::DateTime::parse_from_rfc3339(&req.expires_at).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "INVALID_DATE".to_string(),
                message: "Invalid RFC3339 date format".to_string(),
            }),
        )
            .into_response();
    }

    match cache.set_ttl(&pk, &req.expires_at).await {
        Ok(_) => (
            StatusCode::OK,
            Json(PinResponse {
                pk: pk.clone(),
                message: format!("TTL set successfully for item '{}'", pk),
            }),
        )
            .into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("pinned") {
                (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        error: "CONFLICT".to_string(),
                        message: "Cannot set TTL on a pinned item. Unpin first.".to_string(),
                    }),
                )
                    .into_response()
            } else if error_msg.contains("no rows") {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: "NOT_FOUND".to_string(),
                        message: format!("Item with pk '{}' not found in cache", pk),
                    }),
                )
                    .into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "TTL_ERROR".to_string(),
                        message: format!("Cannot set TTL: {}", e),
                    }),
                )
                    .into_response()
            }
        }
    }
}

/// Supprime le TTL d'un item
///
/// L'item ne sera plus supprimé automatiquement.
pub async fn clear_item_ttl<C: CacheConfig + 'static>(
    State(cache): State<Arc<Cache<C>>>,
    Path(pk): Path<String>,
) -> impl IntoResponse {
    match cache.clear_ttl(&pk).await {
        Ok(_) => (
            StatusCode::OK,
            Json(PinResponse {
                pk: pk.clone(),
                message: format!("TTL cleared successfully for item '{}'", pk),
            }),
        )
            .into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("no rows") {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: "NOT_FOUND".to_string(),
                        message: format!("Item with pk '{}' not found in cache", pk),
                    }),
                )
                    .into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "TTL_ERROR".to_string(),
                        message: format!("Cannot clear TTL: {}", e),
                    }),
                )
                    .into_response()
            }
        }
    }
}
