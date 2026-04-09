# Migration `mdns` → `mdns-sd`

## Contexte

La crate `mdns 3.0.0` (dernière version, 2021, abandonnée) s'appuie sur `dns-parser 0.8.0`
qui rejette les labels DNS non-ASCII avec `LabelIsNotAscii`. Les appareils Apple utilisent
U+2019 (RIGHT SINGLE QUOTATION MARK) dans leurs noms (ex : "Sophie's MacBook Air"), ce qui
spamme les logs en WARN à chaque paquet mDNS reçu.

`mdns-sd 0.19` (avril 2026, activement maintenu) gère l'UTF-8 correctement, fournit une API
de plus haut niveau (service pré-assemblé), et ne nécessite pas `async-std`.

## Périmètre

Deux fichiers à modifier, un fichier à nettoyer :

| Fichier | Rôle |
|---|---|
| `pmocontrol/Cargo.toml` | Dépendances |
| `pmocontrol/src/control_point.rs` | Thread de découverte mDNS (lignes 145–195) |
| `pmocontrol/src/discovery/chromecast_discovery.rs` | Parsing des réponses mDNS |
| `pmoserver/src/logs/mod.rs` | Filtre de bruit `mdns=error` devenu inutile |

---

## Étape 1 — `pmocontrol/Cargo.toml`

### Supprimer
```toml
mdns = "3.0"
```

Vérifier si `async-std` et `futures-util` sont utilisés **ailleurs** que dans le thread mDNS.
D'après l'analyse :
- `async_std` : uniquement `control_point.rs:161` → **supprimer**
- `futures-util` : uniquement `control_point.rs:150,167` → **supprimer**

### Ajouter
```toml
mdns-sd = "0.19"
```

---

## Étape 2 — `pmocontrol/src/control_point.rs`

### Code actuel (lignes 145–195) à remplacer intégralement

```rust
// Thread de découverte mDNS pour Chromecast
let registry_for_mdns = Arc::clone(&registry);
let udn_cache_for_mdns = Arc::clone(&udn_cache);
thread::spawn(move || {
    use crate::discovery::ChromecastDiscoveryManager;
    use futures_util::StreamExt;

    let mut discovery_manager =
        ChromecastDiscoveryManager::new(registry_for_mdns, udn_cache_for_mdns);

    debug!("Starting mDNS discovery thread for Chromecast devices");

    const SERVICE_NAME: &str = "_googlecast._tcp.local";

    async_std::task::block_on(async {
        match mdns::discover::all(SERVICE_NAME, Duration::from_secs(15)) {
            Ok(discovery) => {
                let stream = discovery.listen();
                futures_util::pin_mut!(stream);
                debug!("mDNS discovery stream started for Chromecast devices");
                while let Some(result) = stream.next().await {
                    match result {
                        Ok(response) => {
                            debug!("Received mDNS response with {} records", response.records().count());
                            discovery_manager.handle_mdns_response(response);
                        }
                        Err(e) => { warn!("mDNS discovery error: {}", e); }
                    }
                }
                warn!("mDNS discovery stream ended unexpectedly");
            }
            Err(e) => { error!("Failed to start mDNS discovery: {}", e); }
        }
    });
});
```

### Nouveau code

```rust
// Thread de découverte mDNS pour Chromecast
let registry_for_mdns = Arc::clone(&registry);
let udn_cache_for_mdns = Arc::clone(&udn_cache);
thread::spawn(move || {
    use crate::discovery::ChromecastDiscoveryManager;
    use mdns_sd::{ServiceDaemon, ServiceEvent};

    let mut discovery_manager =
        ChromecastDiscoveryManager::new(registry_for_mdns, udn_cache_for_mdns);

    debug!("Starting mDNS discovery thread for Chromecast devices");

    // Note: mdns-sd requires the trailing dot in the service type
    const SERVICE_TYPE: &str = "_googlecast._tcp.local.";

    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            error!("Failed to create mDNS daemon: {}", e);
            return;
        }
    };

    let receiver = match daemon.browse(SERVICE_TYPE) {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to start mDNS browse for {}: {}", SERVICE_TYPE, e);
            return;
        }
    };

    debug!("mDNS discovery started for Chromecast devices");

    while let Ok(event) = receiver.recv() {
        match event {
            ServiceEvent::ServiceResolved(info) => {
                debug!(
                    fullname = info.get_fullname(),
                    host = info.get_hostname(),
                    port = info.get_port(),
                    "mDNS Chromecast service resolved"
                );
                discovery_manager.handle_service_resolved(&info);
            }
            ServiceEvent::ServiceRemoved(_service_type, fullname) => {
                debug!(fullname = %fullname, "mDNS Chromecast service removed");
                // Pas de retrait actif du registre : le timeout habituel s'en charge
            }
            _ => {}
        }
    }

    warn!("mDNS discovery receiver closed");
});
```

### Points d'attention
- Le point final `.` dans `"_googlecast._tcp.local."` est **obligatoire** pour `mdns-sd`.
- `receiver.recv()` est bloquant synchrone — pas besoin d'async runtime.
- `ServiceDaemon` gère son propre thread interne ; inutile de relancer manuellement.

---

## Étape 3 — `pmocontrol/src/discovery/chromecast_discovery.rs`

### Supprimer l'import `mdns`

```rust
// Supprimer ces uses implicites via le type dans la signature
use std::collections::HashMap;  // <- plus nécessaire si on passe par TxtProperties
```

### Ajouter l'import `mdns-sd`

```rust
use mdns_sd::ServiceInfo;
```

### Remplacer `handle_mdns_response` par `handle_service_resolved`

#### Code actuel (lignes 39–179) — ~80 lignes de parsing manuel

Toute la logique d'extraction PTR / A / AAAA / SRV / TXT disparaît.

#### Nouveau code

```rust
/// Traite un service Chromecast résolu par mDNS-SD.
///
/// `ServiceInfo` arrive pré-assemblé : plus besoin de jointure manuelle
/// des enregistrements PTR / A / SRV / TXT.
pub fn handle_service_resolved(&mut self, info: &ServiceInfo) {
    let fullname = info.get_fullname().to_string();

    debug!("Processing resolved Chromecast service: {}", fullname);

    // Adresse IP : préférer IPv4
    let host = match info
        .get_addresses()
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| info.get_addresses().iter().next())
    {
        Some(addr) => addr.to_string(),
        None => {
            warn!("No IP address for Chromecast service: {}", fullname);
            return;
        }
    };

    let port = info.get_port();

    // TXT records — API directe par clé
    let uuid = info
        .get_property_val_str("id")
        .unwrap_or_default()
        .to_string();
    let uuid = if uuid.is_empty() {
        format!("chromecast-{}-{}", host, port)
    } else {
        uuid
    };

    let model = info.get_property_val_str("md").map(|s| s.to_string());

    let friendly_name = info
        .get_property_val_str("fn")
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Fallback : extraire depuis le fullname, supprimer le suffixe de service
            fullname
                .split("._googlecast._tcp.local")
                .next()
                .unwrap_or("Unknown Chromecast")
                .split('-')
                .take_while(|part| part.len() != 32)
                .collect::<Vec<_>>()
                .join("-")
                .trim()
                .to_string()
        });

    debug!(
        "Discovered Chromecast: {} at {}:{} (UUID: {}, Model: {:?})",
        friendly_name, host, port, uuid, model
    );

    let udn = format!("uuid:{}", uuid);
    let default_max_age = 1800u64;

    if !UDNRegistry::should_fetch(self.udn_cache.clone(), &udn, default_max_age) {
        debug!("Chromecast {} recently seen, skipping", udn);
        return;
    }

    let renderer_info = build_renderer_info(
        &uuid,
        &friendly_name,
        &host,
        port,
        model.as_deref(),
        Some("Google Inc."),
    );

    self.device_registry
        .write()
        .expect("DeviceRegistry mutex lock failed")
        .push_renderer(&renderer_info, default_max_age as u32);
}
```

### Supprimer
- L'import `use std::collections::HashMap` (plus utilisé)
- Tout le bloc `handle_mdns_response` (lignes 39–179)

### Conserver sans modification
- `build_renderer_info` (lignes 182–234)
- `extract_host_from_location` / `extract_port_from_location` (lignes 239–258)
- Les tests (lignes 260–287)

---

## Étape 4 — `pmoserver/src/logs/mod.rs`

Le filtre de bruit `mdns=error` injecté dans `build_filter_with_noise_suppressions` n'est
plus nécessaire. Deux options :

**Option A (recommandée)** — Supprimer l'entrée du tableau :
```rust
const NOISE_FILTERS: &[(&str, &str)] = &[
    // ("mdns", "mdns=error"),  // supprimé : migration vers mdns-sd
];
```
Ou supprimer `build_filter_with_noise_suppressions` entièrement si aucun autre bruit n'est
à filtrer, et revenir à `EnvFilter::try_new(base)` direct.

**Option B** — Laisser en place. La directive `mdns=error` ne cause aucun dommage si la
crate `mdns` n'est plus dans le build (elle sera simplement ignorée).

---

## Résumé des diffs

| Fichier | Lignes supprimées | Lignes ajoutées |
|---|---|---|
| `Cargo.toml` | `mdns`, `async-std`, `futures-util` | `mdns-sd` |
| `control_point.rs` | ~50 (async block) | ~35 (sync recv loop) |
| `chromecast_discovery.rs` | ~80 (parsing manuel) | ~50 (lecture ServiceInfo) |
| `logs/mod.rs` | ~5 (filtre bruit) | 0 |

## Vérification

Après implémentation :
1. `cargo check -p pmocontrol` sans erreurs ni `use of undeclared crate mdns`
2. `cargo check -p pmoserver` sans erreurs
3. Tester la découverte d'un Chromecast en réseau local
4. Vérifier l'absence de `LabelIsNotAscii` dans les logs avec des appareils Apple présents
