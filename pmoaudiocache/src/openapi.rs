//! Documentation OpenAPI pour l'API du cache audio

use utoipa::OpenApi;

/// Documentation OpenAPI pour l'API PMOMusic Audio Cache
///
/// L'API réutilise les handlers génériques de pmocache.
#[derive(OpenApi)]
#[openapi(
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
        (name = "audio", description = "Gestion du cache de pistes audio")
    ),
    info(
        title = "PMOMusic Audio Cache API",
        version = "0.1.0",
        description = r#"
# API de gestion du cache de pistes audio

Cette API permet de gérer un cache de pistes audio avec conversion automatique en FLAC.

## Fonctionnalités

- **Ajout de pistes** : Téléchargement depuis une URL avec conversion automatique en FLAC
- **Métadonnées** : Extraction et stockage automatique des métadonnées audio en JSON
- **Collections** : Organisation par artiste/album
- **Consultation** : Liste des pistes avec statistiques d'utilisation
- **Suppression** : Suppression individuelle ou purge complète
- **Maintenance** : Consolidation du cache pour réparer les incohérences
- **Statut** : Suivi des téléchargements et conversions en cours
- **Streaming progressif** : Les fichiers sont streamés dès qu'ils sont disponibles

## Endpoints principaux

### GET /api/audio
Liste toutes les pistes en cache avec leurs statistiques

### POST /api/audio
Ajoute une piste depuis une URL (conversion FLAC automatique)

### GET /api/audio/{pk}
Récupère les informations complètes d'une piste (avec metadata_json)

### DELETE /api/audio/{pk}
Supprime une piste

### GET /api/audio/{pk}/status
Récupère le statut du téléchargement et de la conversion

### DELETE /api/audio
Purge complètement le cache

### POST /api/audio/consolidate
Consolide le cache (répare les incohérences)

## Servir les fichiers

### GET /audio/flac/{pk}
Récupère le fichier FLAC (streaming progressif si en cours de téléchargement)

### GET /audio/flac/{pk}/orig
Alias pour le fichier original

## Format des fichiers

Les pistes sont stockées au format FLAC avec :
- Une version convertie (`{pk}.orig.flac`)
- Métadonnées stockées en JSON dans la base de données

## Métadonnées

Les métadonnées suivantes sont extraites et stockées :
- Titre, artiste, album
- Année, genre
- Numéro de piste/disque, total de pistes/disques
- Durée, taux d'échantillonnage, bitrate
- Nombre de canaux

## Collections

Les collections sont identifiées par une clé au format `"artist:album"` :
- Conversion en minuscules
- Remplacement des espaces par des underscores
- Exemple : `"Pink Floyd - Wish You Were Here"` → `"pink_floyd:wish_you_were_here"`

## Clés (pk)

Chaque piste est identifiée par une clé (pk) unique :
- Hash SHA1 des 8 premiers octets de l'URL source
- Encodage hexadécimal
- Exemple : `1a2b3c4d5e6f7a8b`

## Statistiques

Le système suit automatiquement :
- Le nombre d'accès (hits)
- La date du dernier accès
- L'URL source originale
- Les métadonnées JSON (accessible via CacheEntry.metadata_json)

## Streaming progressif

Les fichiers en cours de téléchargement sont automatiquement streamés dès que possible :
- Téléchargement asynchrone en arrière-plan
- Conversion FLAC progressive
- Accès aux métadonnées dès le début du téléchargement
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
