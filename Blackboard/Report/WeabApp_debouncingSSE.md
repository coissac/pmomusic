# Rapport : Suppression de la logique de débouncing SSE

**Date**: 2026-01-12
**Tâche**: WeabApp_debouncingSSE.md

## Objectif

Supprimer la logique de débouncing inutile sur le canal SSE de l'application web PMOControl, puisque le serveur contrôle déjà le flux des événements.

## Analyse préalable

J'ai identifié trois endroits avec des mécanismes de temporisation dans l'application web :

### 1. MediaBrowser.vue - Débouncing SSE (À SUPPRIMER ✓)
- **Débouncing**: 200ms après invalidation du cache
- **Cooldown**: 2 secondes entre les rechargements
- **Justification originale**: "dédupliquer les événements SSE dans le même batch (polling 500ms)"
- **Problème**: Cette logique est redondante puisque le serveur contrôle déjà le flux SSE

### 2. useRenderers.ts - Smart fetching (À CONSERVER ✓)
- **Mécanisme**: Comparaison des timestamps `lastEventAt` vs `lastSnapshotAt`
- **But**: Éviter de refetch un snapshot déjà à jour
- **Justification**: Ce n'est PAS du débouncing, c'est une optimisation intelligente qui évite des appels API inutiles

### 3. VolumeControl.vue - UI debouncing (À CONSERVER ✓)
- **Débouncing**: 300ms sur les changements de volume
- **But**: Réduire les appels API pendant que l'utilisateur fait glisser le curseur
- **Justification**: Débouncing légitime pour l'interface utilisateur

## Modifications effectuées

### Fichier modifié: `pmoapp/webapp/src/components/pmocontrol/MediaBrowser.vue`

#### 1. Suppression des variables de débouncing (ligne ~27)

**Avant**:
```typescript
// Flags pour gérer le rechargement automatique avec debounce et cooldown
const isRefreshing = ref(false);
const refreshTimeoutId = ref<number | null>(null);
const lastRefreshTime = ref<number>(0);
const REFRESH_COOLDOWN_MS = 2000; // Ne pas recharger plus d'une fois toutes les 2 secondes
```

**Après**:
```typescript
// Flag pour gérer le rechargement automatique
const isRefreshing = ref(false);
```

#### 2. Simplification du watcher de cache (ligne ~53)

**Avant**:
```typescript
// Recharger automatiquement si le cache est invalidé (ex: après un ContainersUpdated SSE)
// Cela se produit notamment quand on clique sur "Lire maintenant" sur une playlist,
// ce qui déclenche un événement ContainersUpdated qui invalide le cache
// Utilise un debounce de 3 secondes pour regrouper les multiples invalidations
// et un cooldown de 5 secondes pour éviter les rechargements successifs
watch(
    () => browseData.value,
    (data) => {
        if (!data && props.containerId && !loading.value) {
            // Vérifier le cooldown: ignorer si on a rechargé il y a moins de 5 secondes
            const timeSinceLastRefresh = Date.now() - lastRefreshTime.value;
            if (timeSinceLastRefresh < REFRESH_COOLDOWN_MS) {
                console.log(
                    `[MediaBrowser] Cache invalidé mais cooldown actif (${Math.round((REFRESH_COOLDOWN_MS - timeSinceLastRefresh) / 1000)}s restantes), rechargement ignoré`,
                );
                return;
            }

            // Annuler tout timeout en cours
            if (refreshTimeoutId.value !== null) {
                clearTimeout(refreshTimeoutId.value);
            }

            // Planifier le rechargement après 200ms
            refreshTimeoutId.value = window.setTimeout(async () => {
                if (!isRefreshing.value) {
                    console.log(
                        `[MediaBrowser] Cache invalidé pour ${props.serverId}/${props.containerId}, rechargement après debounce...`,
                    );
                    isRefreshing.value = true;
                    await browseContainer(
                        props.serverId,
                        props.containerId,
                        false,
                    );
                    lastRefreshTime.value = Date.now();
                    isRefreshing.value = false;
                    refreshTimeoutId.value = null;
                }
            }, 200);
        }
    },
);
```

**Après**:
```typescript
// Recharger automatiquement si le cache est invalidé (ex: après un ContainersUpdated SSE)
// Cela se produit notamment quand on clique sur "Lire maintenant" sur une playlist,
// ce qui déclenche un événement ContainersUpdated qui invalide le cache
// Le serveur contrôle déjà le flux SSE, pas besoin de debouncing côté client
watch(
    () => browseData.value,
    async (data) => {
        // Si browseData devient undefined alors que containerId est présent,
        // et qu'on n'est pas déjà en train de charger, recharger immédiatement
        if (!data && props.containerId && !loading.value && !isRefreshing.value) {
            console.log(
                `[MediaBrowser] Cache invalidé pour ${props.serverId}/${props.containerId}, rechargement...`,
            );
            isRefreshing.value = true;
            await browseContainer(props.serverId, props.containerId, false);
            isRefreshing.value = false;
        }
    },
);
```

## Résultats

### Changements de comportement
- **Avant**: Délai de 200ms + cooldown de 2s entre les rechargements de cache
- **Après**: Rechargement immédiat dès l'invalidation du cache
- **Impact**: Réactivité améliorée de l'interface, les mises à jour apparaissent immédiatement

### Réduction de complexité
- **3 variables supprimées**: `refreshTimeoutId`, `lastRefreshTime`, `REFRESH_COOLDOWN_MS`
- **Logique simplifiée**: De ~40 lignes à ~10 lignes dans le watcher
- **Code plus lisible**: Intention claire sans mécanismes de temporisation complexes

### Tests
- ✓ Le projet compile sans erreurs TypeScript
- ✓ Le flag `isRefreshing` empêche toujours les rechargements concurrents
- ✓ Les autres composants (useRenderers.ts, VolumeControl.vue) conservent leurs optimisations légitimes

## Conclusion

La suppression du débouncing et du cooldown dans MediaBrowser.vue simplifie le code tout en améliorant la réactivité de l'interface. Puisque le serveur contrôle déjà le flux SSE, ces mécanismes côté client étaient redondants et ajoutaient une latence artificielle.

Le code est maintenant plus simple, plus réactif, et fait confiance au serveur pour contrôler la fréquence des événements SSE.

## Fichiers modifiés
- `pmoapp/webapp/src/components/pmocontrol/MediaBrowser.vue`

## Lignes de code
- **Supprimées**: ~35 lignes (logique de débouncing/cooldown)
- **Ajoutées**: ~5 lignes (logique simplifiée)
- **Net**: -30 lignes
