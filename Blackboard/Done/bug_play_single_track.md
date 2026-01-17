# Synthèse : bug_play_single_track

## Tâche originale

**Crates concernées** : pmocontrol, pmoapp/webapp, pmoqobuz

**Problème** : Lorsque dans le ServerDrawer on clique sur le bouton de lecture d'un item simple Qobuz, rien ne se produit.

**Comportement attendu** :
- Arrêt éventuel du renderer concerné
- Effacement et détachement de sa queue de lecture
- Ajout de la piste sélectionnée dans la queue de lecture
- Lancement de la lecture

---

## Résolution

### Cause racine

`QobuzSource` n'implémentait pas `get_item()`. Quand le ContentDirectory recevait un `BrowseMetadata` sur un track individuel (`qobuz:track:123`), il ne pouvait pas retourner les métadonnées avec une URL HTTP valide.

- **Albums** : `get_or_create_album_playlist_items()` génère des URLs HTTP via le cache (`http://base_url/audio/flac/QOBUZ:123`)
- **Tracks individuels** : URL symbolique `qobuz://track/123` non jouable par le renderer

### Solution

Implémentation de `get_item()` dans `QobuzSource` utilisant `add_track_lazy()` pour enregistrer le track dans le cache et retourner une URL HTTP absolue.

### Fichiers modifiés

| Fichier | Modification |
|---------|--------------|
| `pmoqobuz/src/source.rs` | Ajout de `get_item()` dans l'impl `MusicSource` |
| `pmoqobuz/src/didl.rs` | `format_duration()` rendue publique |

---

## Statut

**Résolu** - Testé et validé.
