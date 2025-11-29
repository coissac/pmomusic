//! Documentation OpenAPI pour l'API REST du cache de couvertures
//!
//! Ce module fournit une documentation OpenAPI simple pour l'API REST
//! fournie par pmocache, spécialisée pour les images de couvertures.

use utoipa::OpenApi;

/// Documentation OpenAPI pour l'API PMOCovers
///
/// L'API réutilise les handlers génériques de pmocache.
#[derive(OpenApi)]
#[openapi(
    paths(
        pmocache::api::list_items::<crate::cache::CoversConfig>,
        pmocache::api::get_item_info::<crate::cache::CoversConfig>,
        pmocache::api::get_download_status::<crate::cache::CoversConfig>,
        pmocache::api::add_item::<crate::cache::CoversConfig>,
        pmocache::api::delete_item::<crate::cache::CoversConfig>,
        pmocache::api::purge_cache::<crate::cache::CoversConfig>,
        pmocache::api::consolidate_cache::<crate::cache::CoversConfig>,
    ),
    components(
        schemas(
            pmocache::CacheEntry,
            pmocache::api::AddItemRequest,
            pmocache::api::AddItemResponse,
            pmocache::api::DeleteItemResponse,
            pmocache::api::ErrorResponse,
            pmocache::api::DownloadStatus,
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
- **Statut** : Suivi des téléchargements en cours

## Endpoints principaux

### GET /api/covers
Liste toutes les images en cache avec leurs statistiques

### POST /api/covers
Ajoute une image depuis une URL (conversion WebP automatique)

### GET /api/covers/{pk}
Récupère les informations d'une image

### DELETE /api/covers/{pk}
Supprime une image et ses variantes

### GET /api/covers/{pk}/status
Récupère le statut du téléchargement

### DELETE /api/covers
Purge complètement le cache

### POST /api/covers/consolidate
Consolide le cache (répare les incohérences)

## Servir les fichiers

### GET /covers/image/{pk}
Récupère l'image originale en WebP

### GET /covers/image/{pk}/{size}
Récupère une variante redimensionnée (ex: /covers/image/abc123/256)

### GET /covers/jpeg/{pk}
Récupère l'image transcodée en JPEG (pour les clients qui ne supportent pas WebP)

### GET /covers/jpeg/{pk}/{size}
Récupère une variante redimensionnée transcodée en JPEG (ex: /covers/jpeg/abc123/256)

## Format des images

Les images sont stockées au format WebP avec :
- Une version originale (`{pk}.orig.webp`)
- Des variantes de tailles générées à la demande (`{pk}.{size}.webp`)

Des routes JPEG sont proposées pour compatibilité UPnP (albumArtURI) :
- `/covers/jpeg/{pk}` (transcodage à la volée depuis le WebP)
- `/covers/jpeg/{pk}/{size}` (transcodage après redimensionnement)

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
