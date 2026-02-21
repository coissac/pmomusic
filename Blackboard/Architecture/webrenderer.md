# WebRenderer UPnP privé par navigateur

## Vue d'ensemble

Le crate `pmowebrenderer` transforme chaque navigateur connecté en un **MediaRenderer UPnP privé**. Quand un navigateur se connecte via WebSocket, le backend Rust crée dynamiquement un device UPnP dédié. Le ControlPoint envoie des commandes SOAP à ce device, et les action handlers les relaient au navigateur via WebSocket. Le navigateur joue l'audio via `<audio>` et renvoie l'état au backend.

## Architecture

```
Browser (Vue.js)               Rust Backend                    ControlPoint
    |                              |                              |
    |-- WS connect --------------->|                              |
    |<-- SessionCreated (token) ---|                              |
    |-- Init (capabilities) ------>|                              |
    |                              |-- register_device() -------->| (Server)
    |                              |   (Device + Services custom) |
    |                              |-- push_renderer() ---------->| (CP registry)
    |                              |                              |
    |                              |<-- SOAP Play (control_handler)
    |<-- Command(Play, uri) -------|   (action handler -> WS)     |
    |-- StateUpdate(Playing) ----->|                              |
    |                              |-- update StateVarInstance -->| (evented -> SSE)
    |                              |                              |
    |-- WS disconnect ------------>|                              |
    |                              |-- device_says_byebye() ----->| (CP registry)
```

## Flux de connexion

1. Le navigateur ouvre une WebSocket vers `/api/webrenderer/ws`
2. Il envoie un message `Init` avec ses capabilities (user_agent, formats supportes)
3. Le backend construit un `Device` UPnP avec des `Service` models custom :
   - AVTransport (Play, Stop, Pause, Seek, SetURI, GetPositionInfo, etc.)
   - RenderingControl (SetVolume, GetVolume, SetMute, GetMute)
   - ConnectionManager (GetProtocolInfo)
4. Chaque Action a un handler qui capture le `mpsc::UnboundedSender<ServerMessage>` du WS
5. Le device est enregistre via `Server::register_device()` (routes SOAP + DEVICE_REGISTRY)
6. Un `RendererInfo` est pousse dans le `DeviceRegistry` du ControlPoint via `push_renderer()`
7. Le backend renvoie un `SessionCreated` avec le token et les infos du renderer

## Decision cle : Services dynamiques (zero changement pmoupnp)

Plutot que de modifier pmoupnp pour permettre l'override de handlers post-creation, on **construit des `Service` models dynamiques** pour chaque session WebSocket :

- Les `StateVariable` statics de pmomediarenderer sont reutilisees via `Arc::clone(&*VAR)`
- De nouvelles `Action` sont creees avec `Action::new()`, configurees avec `set_handler()` puis wrappees en `Arc`
- Les handlers capturent le sender WS et le `SharedState` (clone a chaque appel via `Fn` closure)
- Le `Device` est construit avec ces services custom, puis enregistre normalement

Cela reutilise toute l'infrastructure existante sans modification de pmoupnp ni pmomediarenderer.

## Propagation d'etat bidirectionnelle

### SOAP -> Navigateur (commandes)
Les action handlers des services AVTransport/RenderingControl :
1. Lisent les arguments SOAP depuis `ActionData` via la macro `get!()`
2. Envoient un `ServerMessage::Command` ou `SetVolume`/`SetMute` via le canal mpsc
3. Mettent a jour le `SharedState` local
4. Retournent les arguments OUT via `set!()` si necessaire

### Navigateur -> UPnP (etats)
Quand le navigateur envoie `StateUpdate`, `PositionUpdate`, `MetadataUpdate` ou `VolumeUpdate` :
1. Le `SharedState` est mis a jour
2. Les `StateVarInstance` du `DeviceInstance` sont mises a jour via `set_value(StateValue::...)`
3. Les variables evented declenchent les notifications UPnP captees par le watcher du ControlPoint

## Cycle de vie

- **Connexion** : creation du Device, enregistrement aupres du Server et du ControlPoint
- **Session active** : le `SessionManager` gere un timeout de 30 minutes d'inactivite
- **Deconnexion WS** : appel a `device_says_byebye()` sur le registry du ControlPoint pour marquer offline
- **Cleanup automatique** : le `SessionManager` verifie toutes les 60 secondes les sessions expirees
- **Pas de SSDP** : les WebRenderers sont injectes directement, max_age de 86400s

## Structure des fichiers

| Fichier | Role |
|---------|------|
| `handlers.rs` | Action handlers SOAP->WS (play, stop, pause, seek, set_uri, get_*_info, volume, mute) |
| `renderer.rs` | `WebRendererFactory` : construction dynamique Device/Services avec handlers |
| `websocket.rs` | Handler WS : connexion, reception messages, creation device, propagation etat |
| `session.rs` | `SessionManager` : gestion des sessions avec timeout |
| `config.rs` | `WebRendererExt` trait pour `pmoserver::Server` (enregistrement route WS) |
| `messages.rs` | Types de messages WS (ServerMessage, ClientMessage, etc.) |
| `state.rs` | `RendererState` et `SharedState` (etat partage entre handlers et WS) |
| `error.rs` | Types d'erreur du crate |
| `lib.rs` | Exports publics |

## Points d'attention

1. **parking_lot::RwLockWriteGuard non-Send** : les guards de `SharedState` doivent etre dropes avant tout `.await` dans les handlers async. Utiliser des blocs `{ ... }` pour limiter la portee.

2. **Fn vs FnOnce** : les `ActionHandler` sont `Fn` (appeles plusieurs fois). Les closures doivent cloner `ws` et `state` a chaque appel, avant le `async move`.

3. **Routes Axum persistantes** : Axum ne supporte pas la suppression de routes. Les routes SOAP d'un device deconnecte persistent mais les handlers retournent des erreurs naturellement.

4. **Acces au Server** : le WebSocket handler utilise `pmoserver::get_server()` (singleton global) pour enregistrer les devices dynamiquement.
