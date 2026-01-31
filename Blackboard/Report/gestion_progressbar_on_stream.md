# Rapport : Gestion de la barre de progression sur flux continu

## Résumé

Implémentation complète de l'étape 2 de la tâche : ajout d'un indicateur visuel "Web Radio" dans l'interface web pour signaler la lecture d'un flux continu (webradio). L'indicateur s'affiche automatiquement à côté de l'indicateur "Attachée à une playlist" dans le composant QueueViewer.

## Fichiers modifiés

### Backend (pmocontrol)

1. **pmocontrol/src/openapi.rs**
   - Ajout du champ `is_stream: bool` dans `struct FullRendererSnapshot`

2. **pmocontrol/src/control_point.rs**
   - Modification de la méthode de construction de `FullRendererSnapshot` pour inclure `is_stream` via appel à `renderer.is_playing_a_stream()`

3. **pmocontrol/src/sse.rs**
   - Refactorisation : création de la fonction helper `media_server_event_to_payload()` pour éliminer la duplication de code entre les conversions de `MediaServerEvent` vers `MediaServerEventPayload`
   - Remplacement de deux blocs match dupliqués par des appels à cette fonction helper

### Frontend (webapp)

4. **pmoapp/webapp/src/services/pmocontrol/types.ts**
   - Ajout du type d'événement SSE `stream_state_changed` dans `RendererEventPayload`
   - Ajout du champ `is_stream: boolean` dans `FullRendererSnapshot`

5. **pmoapp/webapp/src/composables/useRenderers.ts**
   - Ajout de la gestion de l'événement `stream_state_changed` dans le switch statement
   - Ajout du computed `isStream` dans le composable `useRenderer()`
   - Export de `isStream` dans le retour du composable

6. **pmoapp/webapp/src/components/pmocontrol/QueueViewer.vue**
   - Import de l'icône `Radio` depuis lucide-vue-next
   - Récupération de `isStream` depuis le composable `useRenderer()`
   - Ajout d'un conteneur `status-indicators` pour wrapper les indicateurs
   - Ajout de l'indicateur visuel "Web Radio" avec icône Radio
   - Ajout des styles CSS pour `.stream-indicator` (badge violet) et `.status-indicators`
