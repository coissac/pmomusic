//! Documentation OpenAPI pour l'API REST du cache de couvertures

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::api::list_images,
        crate::api::get_image_info,
        crate::api::add_image,
        crate::api::delete_image,
        crate::api::purge_cache,
        crate::api::consolidate_cache,
    ),
    components(
        schemas(
            crate::db::CacheEntry,
            crate::api::AddImageRequest,
            crate::api::AddImageResponse,
            crate::api::DeleteImageResponse,
            crate::api::ErrorResponse,
        )
    ),
    tags(
        (name = "covers", description = "Gestion du cache d'images de couvertures")
    ),
    info(
        title = "PMOCovers API",
        version = "0.1.0",
        description = r#"
# API de gestion du cache d'images de couvertures

Cette API permet de gérer un cache d'images optimisé pour les couvertures d'albums.

## Fonctionnalités

- **Ajout d'images** : Téléchargement depuis une URL avec conversion automatique en WebP
- **Consultation** : Liste des images avec statistiques d'utilisation
- **Suppression** : Suppression individuelle ou purge complète
- **Maintenance** : Consolidation du cache pour réparer les incohérences

## Format des images

Les images sont stockées au format WebP avec :
- Une version originale (`{pk}.orig.webp`)
- Des variantes de tailles générées à la demande (`{pk}.{size}.webp`)

## Clés (pk)

Chaque image est identifiée par une clé (pk) unique :
- Hash SHA1 des 8 premiers octets de l'URL source
- Encodage hexadécimal
- Exemple : `1a2b3c4d5e6f7a8b`

## Statistiques

Le système suit automatiquement :
- Le nombre d'accès (hits)
- La date du dernier accès
- L'URL source originale
        "#,
        contact(
            name = "PMOMusic",
        ),
        license(
            name = "MIT",
        ),
    )
)]
pub struct ApiDoc;
