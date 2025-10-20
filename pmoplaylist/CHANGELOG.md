# Changelog

Toutes les modifications notables de ce projet seront documentées dans ce fichier.

Le format est basé sur [Keep a Changelog](https://keepachangelog.com/fr/1.0.0/),
et ce projet adhère au [Semantic Versioning](https://semver.org/lang/fr/).

## [Non publié]

## [0.1.0] - 2025-10-16

### Ajouté

#### Structures de base
- Struct `Track` pour représenter un track audio avec :
  - Identifiant unique
  - Métadonnées (titre, artiste, album, durée)
  - URI du fichier/flux
  - URL optionnelle pour l'image/cover
- Struct `FifoPlaylist` pour gérer une playlist FIFO avec :
  - Capacité configurable
  - Gestion automatique de la rotation (suppression des anciens tracks)
  - Thread-safety via `Arc<RwLock>`
  - Support asynchrone avec tokio

#### Fonctionnalités principales
- **Gestion FIFO** :
  - `append_track()` : Ajoute un track (supprime le plus ancien si capacité atteinte)
  - `remove_oldest()` : Supprime le track le plus ancien
  - `remove_by_id()` : Supprime un track par son ID
  - `clear()` : Vide complètement la playlist
  - `get_items()` : Navigation partielle avec offset/count

- **Détection de changements** :
  - `update_id()` : Compteur incrémenté à chaque modification
  - `last_change()` : Timestamp de la dernière modification
  - Compatibilité avec le protocole UPnP ContentDirectory

- **Génération DIDL-Lite** :
  - `as_container()` : Génère un Container DIDL-Lite pour ContentDirectory
  - `as_container_with_parent()` : Génère un Container avec parent_id personnalisé
  - `as_objects()` : Génère des Items DIDL-Lite avec pagination
  - Mapping complet Track → DIDL Item (métadonnées, ressources, images)

- **Image par défaut** :
  - Image WebP 300x300 intégrée au binaire
  - Note de musique néon sur fond de briques
  - Taille optimisée (~10 KB)
  - Accès via `default_image()`

#### API ergonomique
- Builder pattern pour `Track` :
  - `with_artist()`, `with_album()`, `with_duration()`, `with_image()`
- Méthodes utilitaires :
  - `len()`, `is_empty()`, `id()`, `title()`
- Toutes les méthodes sont asynchrones et thread-safe

#### Documentation
- Documentation complète avec rustdoc
- README.md avec :
  - Guide d'installation
  - Exemples d'utilisation
  - API complète
  - Cas d'usage (radio, album, playlist)
- ARCHITECTURE.md avec :
  - Détails d'implémentation
  - Design patterns utilisés
  - Guide d'intégration
  - Performance et complexité algorithmique

#### Exemples
- `basic_usage.rs` : Utilisation basique de toutes les fonctionnalités
- `radio_streaming.rs` : Simulation d'une radio en streaming multi-thread
- `http_server_integration.rs` : Intégration avec un serveur HTTP

#### Tests
- 11 tests unitaires couvrant :
  - Création et état initial
  - Ajout de tracks
  - Suppression de tracks (oldest, by_id, clear)
  - Navigation et pagination
  - Génération DIDL-Lite
  - Builder pattern
  - Gestion de l'update_id
- 8 doctests intégrés dans la documentation
- 100% de réussite des tests

### Dépendances
- `pmodidl` (local) : Structures DIDL-Lite pour UPnP
- `tokio` 1.42.0 : Runtime asynchrone et RwLock
- `serde` 1.0.228 : Sérialisation de Track

### Notes techniques
- Edition Rust : 2024
- MSRV (Minimum Supported Rust Version) : Non spécifié (version stable recommandée)
- Thread-safe : Oui (Arc + RwLock)
- Async-first : Toutes les méthodes publiques sont async

[Non publié]: https://github.com/user/repo/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/user/repo/releases/tag/v0.1.0
