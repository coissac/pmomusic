# Rapport : Gestion de la barre de progression sur flux continu

## Résumé

Implémentation de l'étape 1 de la tâche : création d'une méthode prédicat `is_playing_a_stream()` au niveau de `MusicRenderer` qui détecte si la lecture en cours est un flux continu (radio) en interrogeant directement les serveurs HTTP.

## Fichiers créés

- `pmocontrol/src/music_renderer/stream_detection.rs` - Module utilitaire pour détecter les flux continus via analyse HTTP

## Fichiers modifiés

### Nouveaux champs `continuous_stream` ajoutés aux backends

1. `pmocontrol/src/music_renderer/upnp_renderer.rs`
   - Ajout du champ `continuous_stream: Arc<Mutex<bool>>`
   - Méthode `is_continuous_stream() -> bool`
   - Détection dans `play_uri()` et `play_from_queue()` via `is_continuous_stream_url()`

2. `pmocontrol/src/music_renderer/openhome_renderer.rs`
   - Ajout des champs `continuous_stream: Arc<Mutex<bool>>` et `current_track_uri: Arc<Mutex<Option<String>>>`
   - Méthode `is_continuous_stream() -> bool`
   - Détection dans `playback_position()` uniquement lors du changement d'URL

3. `pmocontrol/src/music_renderer/linkplay_renderer.rs`
   - Ajout du champ `continuous_stream: Arc<Mutex<bool>>`
   - Méthode `is_continuous_stream() -> bool`
   - Détection dans `play_uri()` via `is_continuous_stream_url()`

4. `pmocontrol/src/music_renderer/arylic_tcp.rs`
   - Ajout du champ `continuous_stream: Arc<Mutex<bool>>`
   - Méthode `is_continuous_stream() -> bool`

5. `pmocontrol/src/music_renderer/chromecast_renderer.rs`
   - Ajout du champ `continuous_stream: Arc<Mutex<bool>>`
   - Méthode `is_continuous_stream() -> bool`
   - Détection dans `play_uri()` via `is_continuous_stream_url()`

### Méthode publique au niveau MusicRenderer

6. `pmocontrol/src/music_renderer/musicrenderer.rs`
   - Ajout de la méthode publique `is_playing_a_stream() -> bool` qui :
     - Vérifie que le renderer est en état `Playing`
     - Interroge le backend pour le statut `continuous_stream`
     - Retourne `false` si non en lecture ou si lecture d'un fichier avec durée

### Export du module

7. `pmocontrol/src/music_renderer/mod.rs`
   - Déclaration du module `stream_detection`
   - Export public de `is_continuous_stream_url`

## Approche technique

### Fonction utilitaire centralisée

La fonction `is_continuous_stream_url(url: &str) -> bool` analyse l'URL en deux étapes :

1. **Pattern matching rapide** : Détection de patterns connus (`/stream`, `/live`, `/radio`, ports 8000/8080, etc.)

2. **Analyse HTTP HEAD** : Si pas de pattern connu, requête HTTP HEAD pour analyser les headers :
   - Headers ICY (Icecast/Shoutcast) → toujours un stream
   - Absence de `Content-Length` + MIME type audio → stream
   - `Transfer-Encoding: chunked` sans `Content-Length` → probablement un stream
   - Présence de `Content-Length` → fichier délimité (non-stream)

### Stratégie par backend

#### Backends sans playlist interne (UPnP, Chromecast, LinkPlay)
- Détection au moment du `play_uri()` car on connaît l'URL qu'on va jouer
- Appel de `is_continuous_stream_url(uri)` pour analyser le serveur HTTP
- Mise à jour du flag `continuous_stream`

#### Backend OpenHome (playlist interne)
- Détection dans `playback_position()` car on ne contrôle pas directement ce qui est joué
- **Uniquement lors du changement d'URL** (détecté via `current_track_uri`)
- Cache l'URL courante pour éviter les vérifications répétées
- Appel de `is_continuous_stream_url()` seulement quand l'URL change

#### Backend ArylicTcp
- Pas de support `play_uri()`, donc flag initialisé mais non utilisé pour l'instant
- Prêt pour extension future si nécessaire

### Points clés de l'implémentation

1. **Détection basée sur le serveur HTTP réel** et non sur les métadonnées DIDL du MediaServer (qui peuvent être segmentées artificiellement)

2. **Optimisation OpenHome** : Vérification uniquement au changement d'URL pour éviter les requêtes HTTP répétées

3. **Timeout de 3 secondes** pour les requêtes HTTP HEAD pour ne pas bloquer

4. **Gestion d'erreur gracieuse** : En cas d'échec de connexion, considère comme non-stream (comportement par défaut sûr)

## Résultat

La méthode `MusicRenderer::is_playing_a_stream()` retourne maintenant `true` si et seulement si :
- Le renderer est en état `Playing` ET
- L'URL en cours de lecture est un flux continu (détecté via analyse HTTP)

Cette implémentation pose les bases pour l'étape 2 qui consistera à utiliser ce prédicat pour gérer correctement la barre de progression sur les flux continus segmentés par métadonnées.
