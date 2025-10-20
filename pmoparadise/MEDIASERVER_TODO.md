# Radio Paradise UPnP Media Server - Plan d'implémentation

## État actuel

Le squelette du module `mediaserver` a été créé mais ne compile pas car il n'utilise pas correctement l'API de pmoupnp.

## Architecture pmoupnp

Après étude du code existant (notamment `pmoupnp/src/mediarenderer/connectionmanager`), voici comment pmoupnp fonctionne :

### 1. Macros à utiliser

Pmoupnp fournit 3 macros essentielles :

```rust
// Définir une variable d'état
define_variable! {
    pub static VAR_NAME: Type = "VariableName" {
        evented: true,  // optionnel
        default: "value", // optionnel
        allowed: ["val1", "val2"], // optionnel
    }
}

// Définir une action
define_action! {
    pub static ACTION_NAME = "ActionName" {
        in "ParamName" => VARIABLE_REF,
        out "ResultName" => RESULT_VAR,
    }
}

// Définir un service
define_service! {
    pub static SERVICE_NAME = "ServiceName" {
        variables: [VAR1, VAR2, ...],
        actions: [ACTION1, ACTION2, ...],
    }
}
```

### 2. Structure des fichiers

Pour chaque service, créer cette structure :

```
src/mediaserver/
├── content_directory/
│   ├── mod.rs                 # Utilise define_service!
│   ├── variables/
│   │   ├── mod.rs
│   │   ├── system_update_id.rs
│   │   ├── container_update_ids.rs
│   │   ├── a_arg_type_objectid.rs
│   │   └── ... (une variable par fichier)
│   └── actions/
│       ├── mod.rs
│       ├── browse.rs          # Utilise define_action!
│       ├── get_search_capabilities.rs
│       └── ...
└── connection_manager/
    └── ... (même structure)
```

### 3. Implémentation de Browse (action complexe)

L'action Browse nécessite un handler custom pour générer le DIDL-Lite dynamiquement :

```rust
// Dans content_directory/actions/browse.rs

use crate::define_action;
use crate::actions::ActionHandler;
use pmoupnp::action_handler;

// Définir les variables d'argument
use super::super::variables::{
    A_ARG_TYPE_OBJECTID,
    A_ARG_TYPE_BROWSEFLAG,
    A_ARG_TYPE_FILTER,
    // ... etc
};

define_action! {
    pub static BROWSE = "Browse" {
        in "ObjectID" => A_ARG_TYPE_OBJECTID,
        in "BrowseFlag" => A_ARG_TYPE_BROWSEFLAG,
        in "Filter" => A_ARG_TYPE_FILTER,
        in "StartingIndex" => A_ARG_TYPE_INDEX,
        in "RequestedCount" => A_ARG_TYPE_COUNT,
        in "SortCriteria" => A_ARG_TYPE_SORTCRITERIA,
        out "Result" => A_ARG_TYPE_RESULT,
        out "NumberReturned" => A_ARG_TYPE_COUNT,
        out "TotalMatches" => A_ARG_TYPE_COUNT,
        out "UpdateID" => A_ARG_TYPE_UPDATEID,
    }
    with handler action_handler!(|instance, data| {
        // Accéder au client Radio Paradise depuis le contexte
        // Générer le DIDL-Lite
        // Retourner les résultats
        Ok(())
    })
}
```

### 4. Contexte pour le client Radio Paradise

Le problème : comment passer `Arc<RwLock<RadioParadiseClient>>` aux handlers ?

**Solution** : Utiliser le `DeviceInstance` pour stocker le client :

```rust
// Dans server.rs

// Créer une structure qui wrappe le client
struct RadioParadiseContext {
    client: Arc<RwLock<RadioParadiseClient>>,
}

// L'attacher au DeviceInstance via son contexte
// (à voir comment pmoupnp gère le contexte custom)
```

Ou alternative : utiliser un registre global thread-safe comme `DEVICE_REGISTRY` dans pmoupnp.

### 5. Intégration pmodidl

Pour générer le DIDL-Lite, il faut utiliser pmodidl correctement :

```rust
// Les types corrects sont :
use pmodidl::{Container, Item, Object};

// Pas DIDLObject, DIDLContainer, etc.

let mut container = Container::new();
container.set_id("0".to_string());
container.set_parent_id("-1".to_string());
container.set_title("Radio Paradise".to_string());

// Sérialiser en XML DIDL-Lite
let didl_xml = container.to_didl();
```

### 6. Intégration pmoserver

Le ServerBuilder de pmoserver prend 3 arguments :

```rust
let server = pmoserver::ServerBuilder::new(
    "RadioParadise",           // name
    "http://localhost:8080",   // base_url
    8080                       // port
).build()?;
```

Pas de méthode `with_port()` - le port est dans le constructeur.

### 7. Méthode Device::set_udn

N'existe pas ! À la place :

```rust
device.set_udn_prefix("uuid:");
// L'UDN complet sera généré automatiquement
```

Ou vérifier s'il faut utiliser `set_uuid()`.

## Plan d'implémentation corrigé

### Phase 1 : ConnectionManager simple (sans handler)

1. Créer `src/mediaserver/connection_manager/mod.rs`
2. Créer les variables dans `connection_manager/variables/*.rs`
3. Créer les actions simples dans `connection_manager/actions/*.rs`
4. Utiliser `define_service!` pour assembler

### Phase 2 : ContentDirectory avec handler

1. Créer la structure de fichiers pour ContentDirectory
2. Implémenter toutes les variables d'argument
3. Implémenter GetSearchCapabilities, GetSortCapabilities (sans handler)
4. Implémenter Browse avec un handler custom
5. Résoudre le problème du contexte (client RP)

### Phase 3 : Serveur principal

1. Corriger `server.rs` pour utiliser la bonne API ServerBuilder
2. Corriger `Device::set_udn`
3. Instancier les services correctement
4. Gérer le cycle de vie du serveur

### Phase 4 : Tests

1. Tester ConnectionManager seul
2. Tester ContentDirectory avec des données mock
3. Tester l'intégration complète
4. Tester avec un client DLNA réel

## Fichiers à modifier

### À supprimer/réécrire complètement :
- `src/mediaserver/content_directory.rs` (approche incorrecte)
- `src/mediaserver/connection_manager.rs` (approche incorrecte)

### À créer :
- `src/mediaserver/connection_manager/mod.rs`
- `src/mediaserver/connection_manager/variables/mod.rs`
- `src/mediaserver/connection_manager/variables/*.rs` (une variable par fichier)
- `src/mediaserver/connection_manager/actions/mod.rs`
- `src/mediaserver/connection_manager/actions/*.rs` (une action par fichier)
- `src/mediaserver/content_directory/` (même structure)

### À corriger :
- `src/mediaserver/server.rs` (API ServerBuilder, Device::set_udn)

## Références

Fichiers pmoupnp à étudier :
- `pmoupnp/src/mediarenderer/connectionmanager/mod.rs` - Exemple complet
- `pmoupnp/src/mediarenderer/connectionmanager/variables/*.rs` - Variables
- `pmoupnp/src/mediarenderer/connectionmanager/actions/*.rs` - Actions
- `pmoupnp/src/services/macros.rs` - Macro define_service!
- `pmoupnp/src/state_variables/macros.rs` - Macro define_variable!
- `pmoupnp/src/actions/macros.rs` - Macro define_action!
- `pmoupnp/src/actions/action_handler.rs` - ActionHandler trait

## Estimation

Temps estimé pour une implémentation correcte :
- Phase 1 (ConnectionManager) : 2-3 heures
- Phase 2 (ContentDirectory) : 4-6 heures
- Phase 3 (Serveur) : 1-2 heures
- Phase 4 (Tests) : 2-3 heures

**Total : 9-14 heures de développement**

## Conclusion

L'implémentation actuelle doit être entièrement réécrite pour utiliser les macros de pmoupnp.
C'est un travail substantiel qui nécessite de bien comprendre l'architecture de pmoupnp avant de commencer.

Le squelette créé (structure de modules, Cargo.toml, exemple) est valide et peut être conservé,
mais tout le code des services doit être réécrit en suivant le pattern de `mediarenderer/connectionmanager`.
