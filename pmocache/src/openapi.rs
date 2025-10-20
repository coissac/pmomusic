//! Génération de documentation OpenAPI pour l'API du cache générique
//!
//! Ce module fournit une macro pour créer dynamiquement la documentation OpenAPI
//! selon le type de cache (images, audio, etc.).

/// Macro pour créer une documentation OpenAPI pour un type de cache
///
/// # Exemple
///
/// ```rust,ignore
/// use pmocache::create_cache_openapi;
///
/// // Génère une struct OpenApi pour le cache de couvertures
/// create_cache_openapi!(
///     CoversApiDoc,
///     "covers",
///     "Covers",
///     "Gestion du cache d'images de couvertures"
/// );
/// ```
#[macro_export]
macro_rules! create_cache_openapi {
    ($doc_name:ident, $cache_name:expr, $cache_title:expr, $cache_description:expr) => {
        #[derive(utoipa::OpenApi)]
        #[openapi(
            paths(
                $crate::api::list_items::<Self>,
                $crate::api::get_item_info::<Self>,
                $crate::api::get_download_status::<Self>,
                $crate::api::add_item::<Self>,
                $crate::api::delete_item::<Self>,
                $crate::api::purge_cache::<Self>,
                $crate::api::consolidate_cache::<Self>,
            ),
            components(
                schemas(
                    $crate::db::CacheEntry,
                    $crate::api::DownloadStatus,
                    $crate::api::AddItemRequest,
                    $crate::api::AddItemResponse,
                    $crate::api::DeleteItemResponse,
                    $crate::api::ErrorResponse,
                )
            ),
            tags(
                (name = $cache_name, description = concat!("Gestion du cache de ", $cache_title))
            ),
            info(
                title = concat!("PMO", $cache_title, " API"),
                version = "0.1.0",
                description = $cache_description,
                contact(
                    name = "PMOMusic",
                ),
                license(
                    name = "MIT",
                ),
            )
        )]
        pub struct $doc_name;
    };
}
