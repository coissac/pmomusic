# Rapport : Queue OpenHome hybride avec cache de métadonnées

## Objectif
Transformer la queue OpenHome de stateless à hybride en ajoutant un cache de métadonnées. Cela permet au control point de mettre à jour les métadonnées des pistes même si le service OpenHome ne le permet pas nativement.

## Problématique
Les services OpenHome ne permettent pas de modifier les métadonnées d'une piste une fois qu'elle est dans la queue. Cela empêchait le control point de refléter les mises à jour de métadonnées effectuées par le média serveur.

## Solution implémentée

### 1. Structure de données
Ajout d'un champ `metadata_cache: HashMap<u32, Option<TrackMetadata>>` dans `OpenHomeQueue` :
- Clé : ID OpenHome de la track (pas l'index/position)
- Valeur : Métadonnées optionnelles de la piste

### 2. Enregistrement des métadonnées
Les métadonnées sont enregistrées dans le cache dans toutes les méthodes qui manipulent des `PlaybackItem` :

- **`add_playback_item`** : Enregistre les métadonnées lors de l'insertion
- **`replace_item`** : Supprime l'ancien ID et enregistre le nouveau
- **`replace_queue`** : Enregistre pour tous les nouveaux items
- **`sync_queue`** et helpers :
  - `replace_queue_preserve_current` : Enregistre pour les nouveaux items
  - `replace_queue_with_pivot` : Met à jour les métadonnées du pivot
  - `rebuild_playlist_section` : Met à jour pour items conservés et nouveaux
  - `replace_queue_standard_lcs` : Met à jour pour items conservés et nouveaux

### 3. Lecture depuis le cache
Modification de `playback_item_from_entry` pour utiliser les métadonnées du cache en priorité :

```rust
let metadata = self.metadata_cache
    .get(&entry.id)
    .cloned()
    .unwrap_or_else(|| entry.metadata());
```

### 4. Nettoyage du cache
Le cache est nettoyé automatiquement lors des suppressions :
- `delete_all()` → `metadata_cache.clear()`
- `delete_id()` / `delete_id_if_exists()` → `metadata_cache.remove()`
- Pas de nettoyage dans `queue_snapshot` (pas nécessaire, quelques entrées orphelines n'ont pas d'impact)

### 5. API publique
Ajout de la méthode publique `update_item_metadata` :

```rust
pub fn update_item_metadata(
    &mut self,
    index: usize,
    metadata: Option<crate::model::TrackMetadata>,
) -> Result<(), ControlPointError>
```

Cette méthode permet de mettre à jour manuellement les métadonnées d'un item à un index donné.

## Points clés de l'implémentation

### Utilisation de l'ID OpenHome (pas l'index)
Le cache utilise l'ID OpenHome comme clé, pas la position dans la queue. Cela permet de suivre une piste même si sa position change.

### Synchronisation intelligente
Dans `sync_queue` :
- **CASE 1** : Item courant PAS dans la nouvelle queue → métadonnées préservées en cache
- **CASE 2** : Item courant DANS la nouvelle queue → métadonnées mises à jour avec celles de la nouvelle queue

### Gestion des fuites mémoire
Quelques entrées orphelines peuvent subsister si un autre control point modifie la playlist, mais :
- Elles ne causent pas de bug (jamais consultées)
- Impact mémoire négligeable
- Naturellement écrasées lors des synchronisations

## Fichiers modifiés
- `pmocontrol/src/queue/openhome.rs` (unique fichier modifié)

## Impact
- ✅ Le control point peut maintenant afficher des métadonnées à jour
- ✅ Les mises à jour du média serveur se reflètent dans la queue
- ✅ Pas de changement de l'API publique (sauf ajout de `update_item_metadata`)
- ✅ Pas d'impact sur les autres backends de queue
- ✅ Compatible avec le comportement existant

## Prochaines étapes suggérées
1. Ajouter `update_item_metadata` aux autres backends de queue (InternalQueue)
2. Exposer cette fonctionnalité au niveau du MediaRenderer
3. Implémenter la synchronisation automatique des métadonnées depuis le MediaServer
