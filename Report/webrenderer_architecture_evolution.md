# Rapport : Évolution architecture webrenderer

## Résumé
Implémentation des phases 0-4 du plan d'évolution : correction du bug P0 (play_handler sans URI), introduction du typage fort avec DeviceCommand/VegearAdapter, integration flac_handle pause/resume, ajout AudioContext avec reconnexion automatique dans PMOPlayer.ts, correction du format de position UPnP, et endpoints JSON nowplaying/state.

## Fichiers modifiés
1. `pmowebrenderer/src/handlers.rs` - Fix P0 + pause/resume flac_handle
2. `pmowebrenderer/src/adapter.rs` - Nouveau fichier avec DeviceCommand et DeviceAdapter
3. `pmowebrenderer/src/state.rs` - VecDeque<DeviceCommand> au lieu de Option<Value>
4. `pmowebrenderer/src/pipeline.rs` - Ajout flac_handle dans PipelineHandle
5. `pmowebrenderer/src/registry.rs` - Format position + get_state + get_state_and_udn
6. `pmowebrenderer/src/register.rs` - Handlers nowplaying et state
7. `pmowebrenderer/src/config.rs` - Nouvelles routes
8. `pmowebrenderer/src/lib.rs` - Exports adapter
9. `pmoapp/webapp/src/services/PMOPlayer.ts` - AudioContext + reconnexion

## Modifications sémantiques
- **P0** : play_handler vérifie URI avant de changement d'état (plus de Transitioning bloqué)
- **P1-P2** : DeviceCommand typé compile-time (plus de serde_json Value non typé)
- **P3** : pause_handler/stop_handler/play_handler appellent flac_handle.pause()/resume()
- **P5** : AudioContext avec createMediaElementSource() et suspend() dans flush()
- **P6** : Reconnexion automatique avec backoff exponentiel
- **P7** : Format position统一 en HH:MM:SS (seconds_to_upnp_time)
- **P8** : Endpoints /nowplaying et /state en JSON
