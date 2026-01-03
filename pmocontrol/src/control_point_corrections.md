# Corrections à appliquer à control_point.rs

## Changements effectués avec succès :

1. ✅ Thread de découverte UPNP : utilise maintenant `UpnpDiscoveryManager` 
2. ✅ Boucle de polling : utilise `Arc<MusicRenderer>` directement avec traits
3. ✅ Suppression du thread OpenHome event forwarder

## Corrections restantes à faire manuellement :

### 1. Thread de vérification des timeouts (ligne ~173)

**Remplacer :**
```rust
let registry_for_presence = Arc::clone(&registry);
let event_bus_for_presence = event_bus.clone();
let media_event_bus_for_presence = media_event_bus.clone();
thread::spawn(move || {
    use ureq::Agent;
    // ... tout le code de vérification HTTP manuelle ...
});
```

**Par :**
```rust
let registry_for_timeout = Arc::clone(&registry);

thread::spawn(move || {
    loop {
        thread::sleep(Duration::from_secs(60));

        // Le registry vérifie les timeouts et émet automatiquement les événements Offline
        if let Ok(mut reg) = registry_for_timeout.write() {
            reg.check_timeouts();
        }
    }
});
```

### 2. Thread Chromecast mDNS (à ajouter après le thread de timeout)

**Ajouter :**
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
                            debug!(
                                "Received mDNS response with {} records",
                                response.records().count()
                            );
                            discovery_manager.handle_mdns_response(response);
                        }
                        Err(e) => {
                            warn!("mDNS discovery error: {}", e);
                        }
                    }
                }

                warn!("mDNS discovery stream ended unexpectedly");
            }
            Err(e) => {
                error!("Failed to start mDNS discovery: {}", e);
            }
        }
    });
});
```

### 3. Correction de la boucle de polling (ligne ~200)

**Le type doit être :**
```rust
let renderers: Vec<Arc<MusicRenderer>> = infos  // Pas Vec<MusicRenderer>
```

### 4. Retour du constructeur (fin de fonction)

**Remplacer :**
```rust
Ok(Self {
    registry,
    event_bus,
    media_event_bus,
    runtime,
})
```

**Par :**
```rust
Ok(Self {
    registry,
    udn_cache,
    event_bus,
    media_event_bus,
    runtime,
})
```

## Suppressions à faire :

- [ ] Supprimer toutes les fonctions OpenHome (à partir de `spawn_openhome_event_runtime`)
- [ ] Supprimer `OpenHomeAccessError` enum
- [ ] Supprimer `OPENHOME_SNAPSHOT_CACHE_TTL` constante
- [ ] Nettoyer les imports obsolètes en haut du fichier

## Note importante :

Le code ne compilera pas tant que tous les imports obsolètes ne seront pas nettoyés, mais la structure sera correcte.
