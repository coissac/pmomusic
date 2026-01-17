# Rapport : bug_play_single_track

## Résumé

Correction du bug empêchant la lecture d'un track Qobuz individuel depuis le ServerDrawer. La cause était l'absence d'implémentation de `get_item()` dans `QobuzSource`, résultant en des URLs symboliques non jouables.

## Travail effectué

1. **Analyse du flux** : Tracé du chemin depuis le clic sur le bouton play (frontend) jusqu'au backend pmocontrol
2. **Identification de la cause** : `QobuzSource` n'implémentait pas `get_item()`, donc les tracks individuels retournaient des URLs symboliques `qobuz://track/{id}` au lieu d'URLs HTTP
3. **Implémentation de la solution** : Ajout de `get_item()` utilisant le même mécanisme de cache lazy que les albums

## Fichiers modifiés

| Fichier | Modification |
|---------|--------------|
| `pmoqobuz/src/source.rs` | Ajout de `get_item()` dans l'impl `MusicSource`, import de `format_duration` |
| `pmoqobuz/src/didl.rs` | `format_duration()` rendue publique |

## Détails techniques

### Cause racine

- **Albums Qobuz** : `get_or_create_album_playlist_items()` crée une playlist avec URLs HTTP absolues (`http://base_url/audio/flac/QOBUZ:123`)
- **Tracks individuels** : `get_item()` non implémenté → fallback échoue → URL symbolique `qobuz://track/123` inutilisable par le renderer

### Solution

`get_item()` :
1. Parse l'object_id pour extraire le track_id
2. Récupère le track via l'API Qobuz
3. Enregistre le track dans le cache avec `add_track_lazy()`
4. Retourne un `Item` avec URL HTTP absolue : `http://base_url/audio/flac/QOBUZ:{track_id}`

## Statut

Bug résolu et testé.
