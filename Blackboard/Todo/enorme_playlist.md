** Ce travail devra être réalisé en suivant scrupuleusement les consignes listées dans le fichier [@Rules_optimal.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules_optimal.md) **

## Contexte et symptôme

Le control point PMOMusic est lent lorsqu'un renderer **OpenHome** manipule des playlists
d'environ 1 000 titres. Les renderers Chromecast et UPnP pur ne sont pas affectés : ils
utilisent une `InternalQueue` entièrement locale, sans appels SOAP. Le problème est
spécifique à `OpenHomeQueue` (`pmocontrol/src/queue/openhome.rs`).

Le code a été généré par IA : il peut contenir des redondances, mais **chaque comportement
est intentionnel**. L'objectif est d'optimiser sans rien supprimer.

## Causes racines identifiées

### P0 — Double appel à `queue_snapshot()` dans `sync_queue()`

**Fichiers** : `pmocontrol/src/queue/openhome.rs`

`sync_queue()` (ligne 1151) appelle `queue_snapshot()` pour obtenir l'état courant.
Puis elle délègue à l'une de ces deux sous-fonctions qui appellent **à nouveau**
`queue_snapshot()` :

- `replace_queue_with_pivot()` (ligne 555) : 2e appel `queue_snapshot()` + 1 appel
  `track_ids()` séparé (alors que `queue_snapshot()` appelle déjà `track_ids()` en interne)
- `replace_queue_standard_lcs()` (ligne 647) : 2e appel `queue_snapshot()`

Seule `replace_queue_preserve_current()` n'a pas ce défaut (elle appelle uniquement
`track_ids()`).

**Impact pour 1 000 titres :**

Chaque `queue_snapshot()` exécute :
- 1 appel SOAP `IdArray` (liste des IDs)
- 16 appels SOAP `ReadList` (lots de 64 items)

Soit **34 appels SOAP** pour une seule opération `sync_queue()` au lieu de 17.

Le cache `ReadList` (TTL 500 ms) atténue partiellement mais ne supprime pas le problème
car la durée d'un `sync_queue` sur 1 000 titres peut dépasser 500 ms.

### P1 — Algorithme LCS de complexité quadratique O(m × n)

**Fichier** : `pmocontrol/src/queue/openhome.rs:851`

La fonction `lcs_flags()` alloue une table DP de taille `(m+1) × (n+1)` :

```rust
let mut dp = vec![vec![0u32; n + 1]; m + 1];
```

Pour 1 000 titres en entrée : 1 000 × 1 000 = **1 000 000 entrées** (≈ 4 MB), et
1 000 000 comparaisons. Elle est appelée **jusqu'à 3 fois** dans un seul `sync_queue` :
- 2 fois dans `replace_queue_with_pivot()` (avant et après le pivot, lignes 579 et 582)
- 1 fois dans `replace_queue_standard_lcs()` (ligne 661)

Dans le cas courant (ajout de titres en fin de liste, ou liste déjà synchronisée),
la quasi-totalité de la table DP est inutile : les préfixe et suffixe communs
représentent souvent 90 % ou plus de la liste.

### P2 — Taille de lot `ReadList` = 64

**Fichier** : `pmocontrol/src/queue/openhome.rs:986`

```rust
const MAX_BATCH: usize = 64;
```

Pour 1 000 titres : 1 000 ÷ 64 = **16 appels SOAP `ReadList`** par `queue_snapshot()`.
La latence réseau typique par appel SOAP (50–200 ms) implique 0,8 à 3,2 secondes
uniquement pour la lecture des métadonnées.

La norme OpenHome Playlist ne fixe pas de limite de payload. La valeur 64 est
conservatrice. Augmenter à 256 réduit à **4 appels** (−75 %).

Le mécanisme de fallback one-by-one (lignes 1007–1019) assure la rétrocompatibilité
avec les devices qui refuseraient un payload plus large.

### P3 — Polling à 500 ms indépendant de l'activité

**Fichier** : `pmocontrol/src/music_renderer/watcher.rs`

Chaque renderer OpenHome tourne un thread watcher toutes les 500 ms, même en veille.
Avec plusieurs renderers actifs, les appels de polling et les opérations `sync_queue`
se chevauchent sur le même device réseau, créant de la contention.

### P4 — Redondances de code (nettoyage conservatif)

**a. Invalidation des caches dupliquée** (`openhome.rs`)

La séquence d'invalidation apparaît en 3 endroits distincts (lignes 1134–1136,
454–456, 634–635) :

```rust
self.track_ids_cache.lock().unwrap().invalidate();
self.read_list_cache.lock().unwrap().invalidate();
// parfois aussi :
self.current_track_id_cache.lock().unwrap().invalidate();
```

**b. Protection durée stream dupliquée** (`openhome.rs` et `interne.rs`)

La logique de protection de durée pour les flux continus (radio) est implémentée :
- Dans `cache_metadata()` de `OpenHomeQueue` (`openhome.rs:250–351`)
- Dans `protect_stream_durations()` de `InternalQueue` (`interne.rs:92–143`)
- Dans `merge_metadata_protecting_streams()` de `InternalQueue` (`interne.rs:148–215`)

**c. `parse_duration()` défini 3 fois**

La conversion `HH:MM:SS` → secondes apparaît dans `openhome.rs`, `interne.rs`,
et dans `time_utils::parse_hhmmss_u32()` (déjà publique).

## Ce qui fonctionne déjà correctement

**Pagination du Browse** : La boucle de pagination est correctement implémentée dans
`control_point.rs:1610–1653` avec `browse_children()` + offset incrémental.

**Fallback ReadList one-by-one** : Si un batch échoue, le retry unitaire (lignes 1007–1019)
assure la robustesse sur les devices stricts.

**Protection multi-control-point** : `delete_id_if_exists()` gère proprement le cas où
un autre control point a déjà supprimé un titre.

**Stratégie double-LCS avec pivot** : La logique de `replace_queue_with_pivot()` est
correcte et importante pour ne pas interrompre la lecture en cours.

**Cache métadonnées stream** : La protection de durée décroissante pour les flux radio
est un comportement essentiel à préserver scrupuleusement.

## Plan d'exécution

### Crate concernée : `pmocontrol`

---

### Étape 1 — Augmenter le batch `ReadList` à 256

**Fichier** : `pmocontrol/src/queue/openhome.rs:986`

```rust
// Avant
const MAX_BATCH: usize = 64;

// Après
const MAX_BATCH: usize = 256;
```

Le fallback one-by-one (lignes 1007–1019) reste intact. Si un renderer refuse
un payload de 256 IDs, il retombe automatiquement sur le mode unitaire.

---

### Étape 2 — Éliminer le double appel à `queue_snapshot()`

**Fichier** : `pmocontrol/src/queue/openhome.rs`

Le snapshot calculé dans `sync_queue()` contient déjà les items **et** leurs IDs
backend (`backend_id: usize`). Il n'est pas nécessaire de le recalculer dans les
sous-fonctions.

#### 2a. Passer le snapshot à `replace_queue_with_pivot()`

Signature actuelle (ligne 548) :
```rust
fn replace_queue_with_pivot(
    &mut self,
    new_items: Vec<PlaybackItem>,
    pivot_idx_new: usize,
    pivot_id: usize,
) -> Result<(), ControlPointError>
```

Nouvelle signature :
```rust
fn replace_queue_with_pivot(
    &mut self,
    new_items: Vec<PlaybackItem>,
    pivot_idx_new: usize,
    pivot_id: usize,
    snapshot: &QueueSnapshot,           // ← ajouté
    current_track_ids: &[u32],          // ← ajouté (évite aussi le 2e appel track_ids())
) -> Result<(), ControlPointError>
```

À l'intérieur de `replace_queue_with_pivot()`, supprimer :
```rust
// Supprimer ces deux lignes (ligne 555–556)
let snapshot = self.queue_snapshot()?;
let current_track_ids = self.track_ids()?;
```

Et utiliser directement les paramètres `snapshot` et `current_track_ids`.

Appel depuis `sync_queue()` (ligne 1221) :
```rust
// Avant
self.replace_queue_with_pivot(items, pivot_idx, playing_id)?;

// Après — passer le snapshot et les IDs déjà disponibles
let current_ids_for_pivot: Vec<u32> = snapshot.items
    .iter()
    .map(|i| i.backend_id as u32)
    .collect();
self.replace_queue_with_pivot(items, pivot_idx, playing_id, &snapshot, &current_ids_for_pivot)?;
```

**Note importante** : dans `sync_queue()`, le snapshot est pris APRÈS
`ensure_playlist_source_selected()` (ligne 1115) et APRÈS la résolution du `playing_info`.
Cet ordre est correct et doit être conservé.

#### 2b. Passer le snapshot à `replace_queue_standard_lcs()`

Signature actuelle (ligne 641) :
```rust
fn replace_queue_standard_lcs(
    &mut self,
    items: Vec<PlaybackItem>,
    _current_index: Option<usize>,
) -> Result<(), ControlPointError>
```

Nouvelle signature :
```rust
fn replace_queue_standard_lcs(
    &mut self,
    items: Vec<PlaybackItem>,
    snapshot: &QueueSnapshot,           // ← ajouté
    current_track_ids: &[u32],          // ← ajouté
) -> Result<(), ControlPointError>
```

À l'intérieur, supprimer :
```rust
// Supprimer ces deux lignes (lignes 647–648)
let snapshot = self.queue_snapshot()?;
let current_track_ids = self.track_ids()?;
```

Appel depuis `sync_queue()` (ligne 1253) :
```rust
// Avant
self.replace_queue_standard_lcs(items, Some(0))?;

// Après
let current_ids_for_lcs: Vec<u32> = snapshot.items
    .iter()
    .map(|i| i.backend_id as u32)
    .collect();
self.replace_queue_standard_lcs(items, &snapshot, &current_ids_for_lcs)?;
```

**Cas particulier à préserver** (ligne 1237–1246) : le guard sur `snapshot.items.is_empty()`
dans `sync_queue()` est exécuté **avant** l'appel à `replace_queue_standard_lcs`, donc
le snapshot vide ne peut pas atteindre la sous-fonction — le comportement est préservé.

---

### Étape 3 — Optimiser LCS par élagage du préfixe/suffixe communs

**Fichier** : `pmocontrol/src/queue/openhome.rs`

La fonction `lcs_flags()` (ligne 851) reste inchangée. L'optimisation s'applique
**aux appels** dans `replace_queue_with_pivot()` et `replace_queue_standard_lcs()`.

#### Principe

Avant de calculer le LCS DP, éliminer les éléments identiques en tête et en queue :

```rust
/// Wrapper autour de lcs_flags() qui élimine préfixe et suffixe communs
/// avant d'appeler l'algorithme DP O(m×n).
/// 
/// Cas optimisés : ajout en fin de liste → O(n), liste déjà synchro → O(n),
/// suppression en fin → O(n). LCS complet uniquement pour les vrais réordonnements.
fn lcs_flags_optimized(
    current: &[PlaybackItem],
    desired: &[PlaybackItem],
) -> (Vec<bool>, Vec<bool>) {
    // Préfixe commun
    let leading = current
        .iter()
        .zip(desired.iter())
        .take_while(|(c, d)| items_match(c, d))
        .count();

    // Suffixe commun (sur les portions restantes uniquement)
    let c_tail = &current[leading..];
    let d_tail = &desired[leading..];
    let trailing = c_tail
        .iter()
        .rev()
        .zip(d_tail.iter().rev())
        .take_while(|(c, d)| items_match(c, d))
        .count();

    let c_mid = &c_tail[..c_tail.len() - trailing];
    let d_mid = &d_tail[..d_tail.len() - trailing];

    // Si rien à faire (listes identiques ou préfixe/suffixe couvrent tout)
    if c_mid.is_empty() && d_mid.is_empty() {
        return (vec![true; current.len()], vec![true; desired.len()]);
    }

    // LCS DP sur le delta central uniquement
    let (keep_c_mid, keep_d_mid) = lcs_flags(c_mid, d_mid);

    // Reconstituer les vecteurs complets
    let mut keep_current = vec![true; leading];
    keep_current.extend(keep_c_mid);
    keep_current.extend(vec![true; trailing]);

    let mut keep_desired = vec![true; leading];
    keep_desired.extend(keep_d_mid);
    keep_desired.extend(vec![true; trailing]);

    (keep_current, keep_desired)
}
```

Remplacer les 3 appels à `lcs_flags()` (lignes 579, 582, 661) par `lcs_flags_optimized()`.

La fonction `lcs_flags()` originale est **conservée** (utilisée en interne par
`lcs_flags_optimized()`).

---

### Étape 4 — Polling adaptatif selon l'activité

**Fichier** : `pmocontrol/src/music_renderer/watcher.rs`

Ajouter un flag partagé `is_active` dans `MusicRenderer` (ou `WatchedState`) pour
signaler si le renderer est en activité récente.

Le renderer met `is_active = true` lors de chaque opération (play, sync, seek, stop).
Le watcher revient à l'intervalle long (5 000 ms) après 10 s sans activité.

```rust
// Dans la boucle du watcher :
let interval = if is_active.load(Ordering::Relaxed) {
    Duration::from_millis(500)
} else {
    Duration::from_millis(5_000)
};
thread::sleep(interval);
```

**Fonctionnalités à préserver** :
- Détection de fin de piste (auto-advance) : délai max 5 s en idle — acceptable
- Sleep timer countdown : reste actif au polling suivant
- Synchronisation auto sur mise à jour de playlist : déclenchée par événement externe,
  pas par le polling — non affectée

---

### Étape 5 — Consolider les redondances (nettoyage conservatif)

**À réaliser uniquement après validation fonctionnelle des étapes 1–4.**

#### 5a. Méthode `invalidate_all_caches()` sur `OpenHomeQueue`

```rust
fn invalidate_all_caches(&self) {
    self.track_ids_cache.lock().unwrap().invalidate();
    self.read_list_cache.lock().unwrap().invalidate();
    self.current_track_id_cache.lock().unwrap().invalidate();
}

fn invalidate_track_caches(&self) {
    self.track_ids_cache.lock().unwrap().invalidate();
    self.read_list_cache.lock().unwrap().invalidate();
}
```

Remplacer les séquences d'invalidation en 3 endroits (lignes 1134–1136, 454–456, 634–635).
Garder les appels sélectifs là où seulement 2 caches sont invalidés.

#### 5b. Factoriser `parse_duration()`

Supprimer les définitions locales de `parse_duration` dans `openhome.rs` et `interne.rs`.
Utiliser `crate::music_renderer::time_utils::parse_hhmmss_u32()` (déjà publique).
La sémantique est identique : conversion `HH:MM:SS` → u64 secondes.

#### 5c. Factoriser la protection durée stream

Extraire la logique commune de protection (« ne jamais diminuer la durée d'un flux
continu pour le même titre/artiste ») dans une fonction privée dans `openhome.rs`,
et y référencer depuis `interne.rs` via le module `queue`.

**Règle absolue** : ne pas modifier la sémantique de détection de stream continu
(`is_continuous_stream_url()`) ni la logique de comparaison titre/artiste. Uniquement
factoriser le code existant.

---

## Ordre d'exécution

1. **Étape 1** — Batch ReadList 256 (changement trivial, gain immédiat −75 % appels)
2. **Étape 2** — Élimination double `queue_snapshot()` (−50 % appels SOAP totaux)
3. **Étape 3** — Optimisation LCS préfixe/suffixe (gain CPU, cas courants en O(n))
4. **Étape 4** — Polling adaptatif (réduction contention réseau en veille)
5. **Étape 5** — Consolidation redondances (nettoyage, après validation)

## Périmètre : ce qui ne change pas

- La logique à 3 cas de `sync_queue()` (avec pivot, préserver courant, LCS standard)
- La protection durée décroissante pour les flux radio (cache stream)
- Le mécanisme `delete_id_if_exists()` pour la robustesse multi-control-point
- Le fallback `ReadList` one-by-one en cas d'erreur batch
- La pagination Browse dans `control_point.rs` (déjà correcte)
- Le comportement des queues `InternalQueue` (Chromecast, UPnP) — non affectées
- Les TTL des caches existants (1 s, 500 ms, 250 ms)
- Tous les logs de diagnostic (`tracing::warn!`, `debug!`) — à conserver

## Tests recommandés

Demander à l'humain de compiler et tester :

```
cargo build -p pmocontrol
```

Puis tester avec un renderer OpenHome physique :
- Playlist de 1 000 titres : mesurer le temps de `sync_queue` avant/après
- Ajout de titres en fin de liste : vérifier que LCS optimisé ne fait que des insertions
- Lecture en cours + refresh playlist : vérifier que la piste courante n'est pas interrompue
- Flux radio : vérifier que la durée ne régresse pas pour un même titre/artiste
- Renderer Chromecast : vérifier l'absence de régression (queue interne)
