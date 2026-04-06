# Rapport : Optimisation performances frontend playlists ~1000 titres

## Résumé
Implémentation de quatre optimisations pour traiter les lenteurs UI observées avec de grandes playlists : virtualisation de la file d'attente avec RecycleScroller, debounce des refetches queue_updated, pagination + cache dans PlayListManager, et fenêtre glissante dans MediaBrowser. Ces changements éliminent les freezes au scroll, bloquages UI et refetches JSON répétés.

## Fichiers modifiés
1. `pmoapp/webapp/src/components/pmocontrol/QueueViewer.vue`
   - Remplacement du v-for natif par RecycleScroller de vue-virtual-scroller
   - Migration de querySelector+scrollIntoView vers scrollToItem() exposé par RecycleScroller

2. `pmoapp/webapp/src/composables/useRenderers.ts`
   - Ajout d'un debounce de 300ms sur les refetches queue_updated pour éviter les refetches en cascade

3. `pmoapp/webapp/src/components/PlayListManager.vue`
   - Ajout pagination client (100 items/page) avec navigation Précédent/Suivant
   -Ajout cache mémorisé pour sortedTracks par playlist ID
   - Simplification lazyTracksCount derivé de sortedTracks

4. `pmoapp/webapp/src/composables/useMediaServers.ts`
   - Ajout fenêtre glissante limitant le cache browse aux 200 derniers items