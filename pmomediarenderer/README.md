# pmomediarenderer

Implémentation d'un MediaRenderer UPnP audio-only conforme à la spécification UPnP AV Architecture.

## Description

Cette crate fournit un MediaRenderer UPnP qui permet de recevoir et lire du contenu audio depuis un serveur UPnP (MediaServer). Elle a été extraite de la crate `pmoupnp` pour permettre une meilleure modularité.

## Architecture

Le MediaRenderer est composé de trois services obligatoires :

- **AVTransport** : Contrôle de la lecture (play, pause, stop, seek, next, previous, etc.)
- **RenderingControl** : Contrôle du volume et du mute
- **ConnectionManager** : Gestion des connexions et des protocoles supportés

## Device UPnP

- Type : `urn:schemas-upnp-org:device:MediaRenderer:1`
- Services : AVTransport:1, RenderingControl:1, ConnectionManager:1

## Utilisation

```rust
use pmomediarenderer::MEDIA_RENDERER;
use pmoupnp::UpnpServer;

// Le device est déjà configuré avec tous ses services
let renderer = MEDIA_RENDERER.clone();

// Créer une instance du renderer
let instance = renderer.create_instance();

// Enregistrer le renderer sur un serveur UPnP
server.register_device(renderer).await?;
```

## Dépendances

- `pmoupnp` : Fournit l'infrastructure UPnP de base (devices, services, actions, state variables)
- `pmodidl` : Pour la gestion des métadonnées DIDL-Lite
- `once_cell` : Pour les initialisations lazy
- `bevy_reflect` : Pour la réflexion et l'introspection

## Services

### AVTransport

Service de contrôle de transport audio conforme UPnP AVTransport:1. Gère la lecture de contenu audio.

**Actions supportées :**
- SetAVTransportURI
- SetNextAVTransportURI
- Play
- Pause
- Stop
- Seek
- Next
- Previous
- GetTransportInfo
- GetPositionInfo
- GetMediaInfo
- GetDeviceCapabilities
- GetTransportSettings
- GetCurrentTransportActions

### RenderingControl

Service de contrôle de rendu conforme UPnP RenderingControl:1. Gère le volume et le mute.

**Actions supportées :**
- GetVolume
- SetVolume
- GetMute
- SetMute

### ConnectionManager

Service de gestion des connexions conforme UPnP ConnectionManager:1. Gère les protocoles supportés.

**Actions supportées :**
- GetProtocolInfo
- GetCurrentConnectionIDs
- GetCurrentConnectionInfo

## Licence

Voir le fichier LICENSE à la racine du projet.
