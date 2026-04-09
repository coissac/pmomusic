# Optimisation Playlist OpenHome — Étape 2

**Contexte**: Suite de `enorme_playlist.md`. Les optimisations de base (MAX_BATCH=256, LCS prefixe/suffixe, polling adaptatif, caches TTL) sont faites. Les lenteurs persistent sur les grandes playlists (~1000 titres). Les renderers Chromecast et UPnP sont moins affectés mais bénéficieront également de certaines optimisations.

**Contrainte architecturale fondamentale**: La queue OpenHome est la **source de vérité unique**. Un miroir local persistent a été tenté et abandonné — impossible à maintenir en sync quand d'autres control points (BubbleUPnP, Linn, etc.) modifient la queue. Toute optimisation doit respecter cette contrainte.

**Note sur le LCS**: `lcs_flags_optimized` (openhome.rs:869) gère déjà le cas dominant (préfixe/suffixe communs). Pour un append de 100 tracks à 900 existants, le LCS est O(1) — ce n'est pas le goulot. Le coût réel est les appels SOAP : connexions TCP × (RTT + handshake) et les `ReadList` pour reconstruire le snapshot.

---

## Analyse des Goulots Réels

Pour un `sync_queue` "append 100 tracks à 900 existants" aujourd'hui :

| Étape | Appels SOAP | Coût estimé |
|-------|------------|-------------|
| `id_array()` | 1 | ~5ms |
| `read_list()` — 4 batches × 256 | 4 | ~20ms |
| 100 × `insert()` | 100 | ~100 × (RTT + **TCP handshake**) |
| **Total TCP handshakes** | 105 | **105 × 10-50ms = 1-5 secondes** |

Les deux leviers : (1) éliminer les handshakes TCP, (2) éliminer les `ReadList` quand inutiles.

---

## Plan d'Implémentation

### Phase 1 — Connection Pooling (1-2h, PRIORITÉ MAXIMALE)

**Fichier**: `pmocontrol/src/soap_client.rs`

**Problème**: lignes 65-72 créent un nouvel `ureq::Agent` à chaque appel SOAP = nouvelle connexion TCP à chaque fois.

**Solution**: Agent statique partagé via `OnceLock`.

```rust
use std::sync::OnceLock;

static SOAP_AGENT: OnceLock<ureq::Agent> = OnceLock::new();

fn get_soap_agent() -> &'static ureq::Agent {
    SOAP_AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into()
    })
}
```

Dans `invoke_upnp_action_with_timeout()`, remplacer la construction de l'agent par :

```rust
// Cas normal : réutiliser l'agent partagé (keep-alive, connection pooling)
// Cas custom timeout : agent dédié (rare — timeout global suffit en pratique)
let agent_owned;
let agent: &ureq::Agent = if timeout.is_some() {
    agent_owned = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(timeout)
        .build()
        .into();
    &agent_owned
} else {
    get_soap_agent()
};
```

**Vérifier** que `ureq::Agent` maintient bien un pool de connexions HTTP/1.1 keep-alive entre les appels (comportement documenté de ureq v3 — l'agent est conçu pour être réutilisé).

**Impact**: Élimine les TCP handshakes répétés pour tous les renderers (OpenHome, UPnP, Chromecast). Pour 100 inserts : 99 handshakes économisés × 10-50ms = **1-5 secondes récupérées**.

---

### Phase 2 — Fast Path Session (2-3h, PRIORITÉ HAUTE)

**Concept**: Sans miroir persistant (abandonné), on peut quand même éviter les `ReadList` dans le cas dominant en maintenant un état **éphémère de session** — valable uniquement entre deux `sync_queue` consécutifs, et invalidé dès qu'on détecte une incohérence.

**Principe**: Après chaque `sync_queue`, mémoriser :
- la liste d'IDs résultante (déjà dans `track_ids_cache` TTL 1s)
- le `after_id` du dernier insert (pour pouvoir appender sans `id_array`)

Au prochain `sync_queue`, tenter de détecter le pattern sans `ReadList` :

```rust
fn try_fast_path(&self, new_items: &[PlaybackItem]) -> FastPathResult {
    // Récupérer les IDs actuels (cache ou 1 appel id_array)
    let current_ids = self.track_ids()?;
    let current_len = current_ids.len();
    let new_len = new_items.len();

    // Fast path 1: append only
    // Condition: new_items a plus d'items, et les current_len premiers de new_items
    // ont les mêmes didl_id que les items actuels (vérifiable depuis id_array + metadata_cache local)
    if new_len > current_len {
        let prefix_matches = self.check_prefix_matches(&current_ids, &new_items[..current_len]);
        if prefix_matches {
            return FastPathResult::AppendOnly { items: &new_items[current_len..] };
        }
    }

    // Fast path 2: delete from end
    if new_len < current_len {
        let prefix_matches = self.check_prefix_matches(&current_ids[..new_len], new_items);
        if prefix_matches {
            let to_delete = &current_ids[new_len..];
            return FastPathResult::DeleteFromEnd { ids: to_delete };
        }
    }

    // Cas général: fallback ReadList + LCS
    FastPathResult::NeedFullSync
}
```

**`check_prefix_matches`** : compare `current_ids[i]` avec `new_items[i]` en utilisant le `metadata_cache` local (déjà en mémoire) pour résoudre les URIs/didl_ids des IDs connus. Si un ID n'est pas en cache → fast path impossible → fallback.

**Clé**: Cette vérification utilise uniquement le `metadata_cache` local (HashMap en mémoire, nano-secondes) et `id_array()` (déjà caché TTL 1s). Zéro appel `ReadList` dans le cas heureux. Si la vérification échoue (incohérence détectée, cache manquant) → fallback propre vers `ReadList` + LCS, source de vérité OpenHome préservée.

**Fichier**: `pmocontrol/src/queue/openhome.rs`, ajouter `try_fast_path()` et l'intégrer en début de `sync_queue()`.

---

### Phase 3 — Queue FIFO Async (8-12h, PRIORITÉ MOYENNE)

**Prérequis**: Phases 1 et 2 complétées.

**Concept**: Exécuter les opérations SOAP dans un thread dédié pour rendre `sync_queue()` non-bloquant du point de vue de l'appelant.

**Contrainte technique**: Les `insert()` sont chaînés — chaque appel retourne un `new_id` utilisé comme `after_id` du suivant. Le worker doit maintenir cet état interne.

**Fichier à créer**: `pmocontrol/src/queue/openhome_op_queue.rs`

```rust
pub enum OpenHomeOp {
    /// Insert séquentiel — after_id géré en interne (last_inserted_id)
    InsertAtEnd { uri: String, metadata: String, didl_id: String },
    /// Insert après un ID connu (ex: après le pivot)
    InsertAfter { after_id: u32, uri: String, metadata: String, didl_id: String },
    DeleteId { track_id: u32 },
    DeleteAll,
    SeekId { id: u32 },
    Play,
    Pause,
    Stop,
    SetVolume { volume: u16 },
}

pub struct OpenHomeOpQueue {
    sender: mpsc::Sender<OpenHomeOp>,
    last_error: Arc<Mutex<Option<ControlPointError>>>,
    completion: Arc<(Mutex<bool>, Condvar)>,
}

impl OpenHomeOpQueue {
    pub fn push(&self, op: OpenHomeOp) { ... }
    /// Opérations critiques passent devant (play/stop/volume)
    pub fn push_priority(&self, op: OpenHomeOp) { ... }
    /// Vider la file d'attente (ex: nouvelle playlist demandée avant fin de sync)
    pub fn clear_pending(&self) { ... }
    /// Attendre que toutes les opérations soient exécutées
    pub fn wait_completion(&self) { ... }
    /// Récupérer la dernière erreur (non-bloquant)
    pub fn take_error(&self) -> Option<ControlPointError> { ... }
}
```

**Comportement en cas d'erreur**: vider la file d'attente, signaler l'erreur via `last_error`, invalider les caches OpenHome (forcer re-sync depuis source de vérité au prochain appel).

**Comportement en cas de `clear_pending()` pendant exécution**: laisser l'opération en cours se terminer (plus sûr — évite de laisser le renderer dans un état inconsistant), vider le reste.

**Intégration dans `OpenHomeQueue`**: remplacer les appels directs `playlist_client.insert()` / `playlist_client.delete_id()` par des `op_queue.push()`. Les opérations qui ont besoin d'une réponse synchrone (ex: `current_track()`, `queue_snapshot()`) continuent d'appeler directement le `playlist_client` — mais doivent d'abord attendre la complétion de la file (`wait_completion()`).

---

### Phase 4 — Throttle des replace_item (2-3h, PRIORITÉ BASSE)

**Contexte**: `replace_item()` (openhome.rs:1315) fait `delete_id` + `insert` pour mettre à jour une piste. Sur un stream radio qui change de morceau, la durée est mise à jour fréquemment → paires SOAP inutiles car le `metadata_cache` local est déjà la source pour l'UI.

**Solution**: Ne pas envoyer le `delete_id` + `insert` OpenHome si une mise à jour pour ce `track_id` a déjà été envoyée dans les N dernières secondes. Le `metadata_cache` local est mis à jour immédiatement (pour l'UI), et l'opération OpenHome est différée ou ignorée.

```rust
fn replace_item(&mut self, index: usize, item: PlaybackItem) -> Result<(), ControlPointError> {
    let track_id = self.track_ids()?[index];

    // Toujours mettre à jour le cache local immédiatement (pour l'UI)
    self.cache_metadata(track_id, item.metadata.clone());

    // Throttle: si replace OpenHome récent pour ce track, sauter l'opération SOAP
    if self.is_recent_replace(track_id, Duration::from_secs(5)) {
        return Ok(());
    }
    self.mark_replace_time(track_id);

    // Opération SOAP (delete + insert)
    // ... code existant ...
}
```

Ajouter `last_replace_times: Mutex<HashMap<u32, SystemTime>>` dans `OpenHomeQueue`.

---

## Ordre d'Implémentation

```
Phase 1 → mesurer gain TCP → Phase 2 → mesurer gain ReadList → Phase 3 → Phase 4
```

Tester chaque phase avec des playlists réelles (~1000 tracks) avant de poursuivre. Ne pas combiner les phases pour pouvoir isoler les régressions.

## Tests

```bash
# Phase 1 — vérifier connexions TCP réutilisées
# tcpdump -i lo port 60000 -c 200  (ou port du renderer)
# Avant: SYN à chaque appel SOAP
# Après: SYN unique, flux keep-alive

# Phase 2 — vérifier fast path activé
# RUST_LOG=debug cargo run ... 2>&1 | grep "fast path"
# Cas append 100 → "fast path: AppendOnly, 100 inserts"
# Cas delete 100 → "fast path: DeleteFromEnd, 100 deletes"
# Cas reorder   → "fast path: NeedFullSync, falling back to ReadList+LCS"

# Phase 3 — vérifier non-blocage
# sync_queue() doit retourner en <1ms (les 100 inserts continuent en background)
```

---

*Contrainte architecturale intégrée: miroir local abandonné (désynchronisation avec autres control points). Toutes les phases respectent OpenHome comme source de vérité unique.*

*Date: 2026-04-08*
