# pmomediaserver

Implémentation d'un MediaServer UPnP conforme à la spécification UPnP AV Architecture.

## Description

Cette crate fournit un MediaServer UPnP qui permet d'exposer et de servir du contenu audio à des clients UPnP (MediaRenderer). Elle suit le même modèle architectural que la crate `pmomediarenderer`.

## Architecture

Le MediaServer est composé de deux services obligatoires :

- **ContentDirectory** : Gestion du contenu et de la navigation dans la bibliothèque musicale
- **ConnectionManager** : Gestion des connexions et des protocoles supportés

## Device UPnP

- Type : `urn:schemas-upnp-org:device:MediaServer:1`
- Services : ContentDirectory:1, ConnectionManager:1

## Utilisation

```rust
use pmomediaserver::MEDIA_SERVER;
use pmoupnp::UpnpServer;

// Le device est déjà configuré avec tous ses services
let server = MEDIA_SERVER.clone();

// Créer une instance du server
let instance = server.create_instance();

// Enregistrer le server sur un serveur UPnP
upnp_server.register_device(server).await?;
```

## Dépendances

- `pmoupnp` : Fournit l'infrastructure UPnP de base (devices, services, actions, state variables)
- `pmodidl` : Pour la gestion des métadonnées DIDL-Lite
- `once_cell` : Pour les initialisations lazy
- `bevy_reflect` : Pour la réflexion et l'introspection

## Services

### ContentDirectory

Service de gestion du contenu conforme UPnP ContentDirectory:1. Permet de naviguer et rechercher dans la bibliothèque musicale.

**Actions supportées :**
- Browse
- Search
- GetSearchCapabilities
- GetSortCapabilities
- GetSystemUpdateID
- CreateObject (optionnel)
- DestroyObject (optionnel)
- UpdateObject (optionnel)

### ConnectionManager

Service de gestion des connexions conforme UPnP ConnectionManager:1. Gère les protocoles supportés.

**Actions supportées :**
- GetProtocolInfo
- GetCurrentConnectionIDs
- GetCurrentConnectionInfo
- PrepareForConnection (optionnel)
- ConnectionComplete (optionnel)

## Licence

Voir le fichier LICENSE à la racine du projet.
