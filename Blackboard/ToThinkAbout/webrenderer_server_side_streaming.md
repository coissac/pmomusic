# WebRenderer : Streaming Audio Côté Serveur

## Problème actuel

Le webrenderer actuel délègue la lecture audio au navigateur : le serveur envoie une URL
de fichier via WebSocket (`SetUri`), et le navigateur charge cette URL dans un élément
`<audio>`. Cette approche a plusieurs limitations :

- Les URLs sont internes (IP locale + port) → inaccessibles depuis l'extérieur
- Les fichiers sur partage Samba ont des chemins locaux → jamais accessibles au navigateur
- La `base_url` doit être configurée statiquement → pas de solution propre local/externe

## Solution proposée : flux HTTP serveur

Le serveur génère un flux audio continu par instance de webrenderer, servi sur un endpoint
HTTP dédié. Le navigateur n'écoute que ce flux — une URL fixe, toujours accessible.

```
Avant :  ControlPoint → SetAVTransportURI(url_interne) → WebSocket → Browser(<audio src=url_interne>)
Après :  ControlPoint → SetAVTransportURI(url_interne) → Serveur(ouvre+stream) → Browser(<audio src=/api/webrenderer/{id}/stream>)
```

## Architecture cible

### Cycle de vie d'une instance

```
1. Navigateur ouvre la page
       ↓
2. POST /api/webrenderer/register  {instance_id, user_agent}
       ↓
3. Serveur crée le device UPnP + pipeline audio
   Annonce SSDP → ControlPoints découvrent le renderer
   Répond : { stream_url }
       ↓
4. Navigateur ouvre GET /api/webrenderer/{id}/stream  (flux FLAC)
       ↓
5. ControlPoint → SetAVTransportURI + Play → pipeline démarre
   Navigateur écoute le flux FLAC en continu
   SSE global existant → métadonnées et état vers l'interface
       ↓
6. Navigateur ferme la page → flux FLAC se coupe
   Serveur détecte → SSDP byebye → pipeline stoppé
   Device UPnP retiré
```

Le lecteur web est complètement invisible — l'interface est pilotée par le SSE global
existant du ControlPoint. Le WebSocket est supprimé. Pas de SSE dédié au webrenderer.

### Endpoints HTTP

```
POST /api/webrenderer/register
     Body: { instance_id, user_agent }
     Réponse: { stream_url }

GET  /api/webrenderer/{id}/stream
     Content-Type: audio/flac
     Cache-Control: no-store, no-transform
     [Flux FLAC continu — déconnexion = fin de session]

DELETE /api/webrenderer/{id}
     Désenregistrement explicite (optionnel, fallback sur coupure du flux)
```

La `stream_url` est une URL relative (`/api/webrenderer/{id}/stream`) — le navigateur
la résout lui-même, toujours correcte en local et via proxy externe, sans reconstruction
depuis les headers `X-Forwarded-*`.

### Composants nécessaires

#### 1. Pipeline audio par instance

Chaque instance possède :

- Un **`StreamingFlacSink`** — infrastructure existante dans `pmoaudio-ext`
- Un **`StreamHandle`** — exposé via l'endpoint `/stream`
- Un canal de contrôle **`PipelineControl`** — alimenté par les actions UPnP

Le pipeline est créé au `POST /register` et détruit à la coupure du flux FLAC.

#### 2. Enregistrement et création du device UPnP

```
POST /api/webrenderer/register
  → créer DeviceInstance UPnP (même factory qu'aujourd'hui)
  → annoncer via SSDP (nouveau : aujourd'hui pas de SSDP pour le webrenderer)
  → créer StreamingFlacSink + pipeline
  → enregistrer dans le RendererRegistry
  → retourner stream_url
```

L'`instance_id` vient du `localStorage` du navigateur — stable entre les reloads,
garantit que le même renderer UPnP est retrouvé à la reconnexion.

#### 3. Modification de `SetAVTransportURI`

Au lieu d'envoyer l'URL au navigateur, le handler UPnP :

1. Reçoit l'URI source (fichier cache, Samba, URL externe...)
2. Envoie `PipelineControl::LoadUri(uri)` au pipeline de l'instance
3. Le pipeline ouvre la source côté serveur et alimente le `StreamingFlacSink`
4. Le navigateur reçoit un event SSE `state_changed: Transitioning` puis `Playing`

#### 4. Gestion des transitions (gapless)

Le `StreamingFlacSink` diffuse un flux FLAC continu. À la frontière de piste, le pipeline
enchaîne les sources sans interruption du flux HTTP.

`SetNextAVTransportURI` → `PipelineControl::LoadNextUri(uri)` → pré-chargé dans le pipeline
→ transition seamless, le navigateur ne recharge pas l'URL.

#### 5. Métadonnées et état

Tout passe par le SSE existant — titre, artiste, artwork, position, état de lecture.
Pas de nouveau mécanisme nécessaire.

#### 6. Seek

Flux HTTP live → pas de Range requests.

Pour les fichiers (non-live) :
- `PipelineControl::Seek(position_sec)` → pipeline repart depuis la nouvelle position
- Légère interruption du flux FLAC (rebuffering navigateur ~1s) — acceptable

#### 7. Sources supportées

Le pipeline réutilise `pmoaudio-ext` et `pmoflac`. La seule source actuellement
déclarée dans `pmoaudio-ext` fonctionne à partir d'une `pmoplaylist` — c'est le
modèle à suivre pour construire dans `source_loader.rs` une source ad-hoc capable
d'ouvrir des URIs arbitraires (URL HTTP externe, fichier local/Samba) qui ne passent
pas par le cache.

### État partagé par instance

```rust
pub struct WebRendererServerState {
    pub playback_state: PlaybackState,
    pub current_uri: Option<String>,
    pub volume: u16,
    pub mute: bool,
    pub stream_handle: SharedStreamHandle,   // Handle vers le flux FLAC
    pub pipeline_tx: mpsc::Sender<PipelineControl>,  // Contrôle du pipeline
}

pub enum PipelineControl {
    LoadUri(String),
    LoadNextUri(String),
    Play,
    Pause,
    Stop,
    Seek(f64),
    SetVolume(u16),
}
```

## Fichiers à créer / modifier

### Nouveaux fichiers

| Fichier | Rôle |
|---------|------|
| `pmowebrenderer/src/stream.rs` | Handler HTTP du flux FLAC |
| `pmowebrenderer/src/pipeline.rs` | Pipeline audio serveur par instance |
| `pmowebrenderer/src/source_loader.rs` | Ouverture des sources (cache, HTTP, fichier local) |
| `pmowebrenderer/src/register.rs` | Handler `POST /register` + `DELETE /{id}` |

### Fichiers à modifier

| Fichier | Modification |
|---------|--------------|
| `pmowebrenderer/src/state.rs` | Ajouter `stream_handle` et `pipeline_tx` |
| `pmowebrenderer/src/handlers.rs` | `set_uri_handler` → `PipelineControl::LoadUri` |
| `pmowebrenderer/src/websocket.rs` | **Supprimer** — remplacé par `register.rs` |
| `pmowebrenderer/src/config.rs` | Enregistrer les nouvelles routes, supprimer WS |
| `pmowebrenderer/src/messages.rs` | **Supprimer** — plus de WebSocket |
| `pmowebrenderer/src/session.rs` | Adapter : session liée au flux FLAC, pas au WS |
| `pmoapp/webapp/src/` | Remplacer WS par `POST /register` + `<audio src=stream_url>` |

## Infrastructure existante réutilisable

Tout le travail difficile est déjà fait :

- **`StreamingFlacSink`** (`pmoaudio-ext/src/sinks/streaming_flac_sink.rs`) — broadcast multi-clients, gestion backpressure, ICY metadata
- **`timed_broadcast`** (`pmoaudio-ext/src/sinks/timed_broadcast.rs`) — diffusion multi-clients avec pacing
- **`pmoflac`** — décodage/encodage FLAC temps réel
- Pattern HTTP streaming (`pmomediaserver/src/paradise_streaming.rs`) — exemple complet à suivre

## Questions ouvertes

1. **Volume côté serveur ou navigateur** ? Actuellement le navigateur gère le volume.
   Côté serveur, on pourrait appliquer un gain DSP dans le pipeline.

3. **Plusieurs onglets simultanés** : chaque onglet a son propre pipeline et flux ?
   Ou partage d'un flux broadcast si même contenu ? (Complexe, probablement un pipeline par instance.)

4. **Reconnexion** : si le navigateur se reconnecte (reload), le pipeline continue de
   tourner ? Ou on le stoppe et recrée ? La `StreamHandle` permet plusieurs souscripteurs,
   donc la reconnexion peut se faire sans interruption.

5. **Latence** : le pipeline serveur introduit une latence (buffering + encodage).
   À mesurer et potentiellement configurer via `max_lead_seconds`.
