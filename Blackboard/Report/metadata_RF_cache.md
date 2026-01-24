# Rapport: Simplification de pmoradiofrance

## Résumé

Refactoring complet de la crate `pmoradiofrance` pour simplifier l'architecture autour d'un cache de métadonnées centralisé avec système d'événements.

## Objectifs

1. Simplifier les structures de stations (supprimer StationType)
2. Créer un cache de métadonnées in-memory avec TTL basé sur `end_time`
3. Maintenir le cache de stations persistant (pmoconfig, TTL 1 semaine)
4. Implémenter un système d'événements pour la synchronisation GENA
5. Unifier les méthodes `to_didl()` pour retourner des Containers DIDL
6. Gérer automatiquement le cache des covers via pmocovers

## Changements architecturaux majeurs

### 1. Nouveau fichier: metadata_cache.rs

**Créé**: `pmoradiofrance/src/metadata_cache.rs`

Contient deux structures principales:

- **CachedMetadata**: Stocke uniquement les données nécessaires au DIDL (titre, artiste, album, cover, stream URL, etc.)
- **MetadataCache**: Gère le cache in-memory avec TTL + cache persistant des stations + système d'événements

**Fonctionnalités**:
- TTL basé sur `end_time` de l'API Radio France
- Gestion automatique du cache de covers via pmocovers
- Système subscribe/notify pour les événements
- Graceful degradation si API Radio France down
- Méthode `to_didl()` retournant une playlist à un item avec métadonnées identiques

### 2. Suppression: stateful_client.rs

**Supprimé**: `pmoradiofrance/src/stateful_client.rs`

Raison: Complètement redondant avec `MetadataCache`. Toute la logique a été déplacée dans le nouveau module.

### 3. Simplification: models.rs

**Modifications**:
- Supprimé `StationType` enum
- Simplifié `Station` struct (juste `slug` + `name`)
- Supprimé méthodes `is_main()`, `is_webradio()`, `is_local_radio()`, `base_station()`
- Conservé structures d'API (`LiveResponse`, `ShowMetadata`, etc.)

### 4. Simplification: playlist.rs

**Modifications**:
- Supprimé `StationPlaylist` complètement
- Simplifié `StationGroup` et `StationGroups`
- **Important**: `to_didl()` retourne `Container` (pas `Vec<Container>`)
- Logique unifiée: ICI fonctionne comme FIP (plus de traitement spécial)
- Préservé les règles de mapping RF → UPnP existantes

### 5. Refactoring: source.rs

**Modifications**:
- Utilise uniquement `MetadataCache` (plus de `stateful_client`)
- Simplifié `browse()` en 3 cas simples
- Abonnement aux événements du cache pour GENA
- Retourne des `Container` (cohérence avec to_didl)

### 6. Adaptation: config_ext.rs

**Modifications**:
- Format simplifié: `Vec<Station>` au lieu de `CachedStationList`
- TTL reste à 7 jours (1 semaine)

### 7. Mise à jour: lib.rs

**Modifications**:
- Ajouté `pub mod metadata_cache;`
- Supprimé export de `stateful_client`
- Ajouté exports: `MetadataCache`, `CachedMetadata`

## Hiérarchie de browse

**Niveau 0**: `radiofrance`
- Retourne UN Container contenant les containers de groupes
- Exemple: Container "FIP", Container "France Culture", Container "ICI"

**Niveau 1**: `radiofrance:group:fip` ou `radiofrance:ici`
- Si 1 station: retourne directement la playlist (Container playlistContainer)
- Si plusieurs stations: retourne un container contenant les playlists

**Niveau 2**: `radiofrance:fip`
- Retourne Container playlistContainer avec 1 item
- Métadonnées identiques entre playlist et item

## Règles de mapping préservées

Les règles existantes de transformation RF → UPnP ont été préservées:
- Radio musicale avec song → métadonnées du morceau
- Radio parlée → agrégation émission/producteur
- Éviter duplications du nom de station
- Calcul de duration depuis end_time

## Système d'événements

**Flux**:
1. `MetadataCache` rafraîchit les métadonnées d'un slug
2. Notifie tous les abonnés via `notify(slug)`
3. `RadioFranceSource` reçoit l'événement
4. Émet un événement GENA UPnP pour la playlist `radiofrance:{slug}`
5. Le Control Point reçoit la notification et peut se mettre à jour

## Fichiers modifiés

### Créés
- `pmoradiofrance/src/metadata_cache.rs`

### Supprimés
- `pmoradiofrance/src/stateful_client.rs`

### Modifiés
- `pmoradiofrance/src/models.rs`
- `pmoradiofrance/src/playlist.rs`
- `pmoradiofrance/src/source.rs`
- `pmoradiofrance/src/config_ext.rs`
- `pmoradiofrance/src/lib.rs`

### Inchangés
- `pmoradiofrance/src/client.rs`
- `pmoradiofrance/src/error.rs`

## Points de vigilance

1. **Migration**: Le cache pmoconfig existant sera invalidé (nouveau format)
2. **Covers**: Nécessite que pmocovers soit initialisé via cache_registry
3. **Thread safety**: Utilisation d'Arc<RwLock> pour la sécurité thread
4. **Graceful degradation**: Retourne cache expiré si API Radio France down

## Prochaines étapes

1. Tester le cache de métadonnées (TTL, refresh, graceful degradation)
2. Tester le système d'événements
3. Tester le browse sur les 3 niveaux
4. Vérifier les événements GENA
5. Vérifier que les covers sont correctement cachées

## Plan d'implémentation détaillé

Le plan détaillé est disponible dans:
`/Users/coissac/.claude/plans/glowing-scribbling-cook.md`
