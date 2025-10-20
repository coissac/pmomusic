# API d'introspection UPnP

## Vue d'ensemble

L'API d'introspection UPnP permet d'explorer et de modifier en temps réel la hiérarchie Device/Service/Action/Variable du serveur UPnP via des endpoints REST.

## Architecture

```text
DeviceRegistry (thread_local)
├── DeviceInstanceSet (indexé par nom)
└── Index UDN → nom

UpnpServer trait (pour pmoserver::Server)
├── register_device()  - Enregistre un device
├── device_count()     - Nombre de devices
├── list_devices()     - Liste tous les devices
└── get_device(udn)    - Récupère par UDN

UpnpApiExt trait (pour pmoserver::Server)
└── register_upnp_api() - Monte l'API REST
```

## Utilisation dans PMOMusic

```rust
use pmoupnp::{UpnpServer, upnp_api::UpnpApiExt};

let mut server = ServerBuilder::new_configured().build();

// Enregistrer l'API d'introspection
server.register_upnp_api().await;

// Enregistrer un device UPnP
server.register_device(MEDIA_RENDERER.clone()).await?;
```

## Endpoints REST disponibles

### Liste tous les devices

```http
GET /api/upnp/devices
```

**Réponse :**
```json
{
  "count": 1,
  "devices": [
    {
      "udn": "uuid:f9ef6c21-0ed3-470c-9846-bc1ae85fea62",
      "name": "MediaRenderer",
      "friendly_name": "PMOMusic MediaRenderer",
      "device_type": "urn:schemas-upnp-org:device:MediaRenderer:1",
      "manufacturer": "PMOMusic",
      "model_name": "MediaRenderer",
      "base_url": "http://192.168.1.100:8080",
      "description_url": "http://192.168.1.100:8080/device/uuid:f9ef6c21.../desc.xml"
    }
  ]
}
```

### Détails d'un device

```http
GET /api/upnp/devices/:udn
```

**Réponse :**
```json
{
  "udn": "uuid:f9ef6c21-0ed3-470c-9846-bc1ae85fea62",
  "name": "MediaRenderer",
  "friendly_name": "PMOMusic MediaRenderer",
  "device_type": "urn:schemas-upnp-org:device:MediaRenderer:1",
  "manufacturer": "PMOMusic",
  "model_name": "MediaRenderer",
  "base_url": "http://192.168.1.100:8080",
  "description_url": "http://192.168.1.100:8080/device/uuid:f9ef6c21.../desc.xml",
  "services": [
    {
      "name": "AVTransport",
      "service_type": "urn:schemas-upnp-org:service:AVTransport:1",
      "service_id": "urn:upnp-org:serviceId:AVTransport",
      "control_url": "http://192.168.1.100:8080/.../control",
      "event_url": "http://192.168.1.100:8080/.../event",
      "scpd_url": "http://192.168.1.100:8080/.../desc.xml"
    }
  ]
}
```

### Variables d'un service

```http
GET /api/upnp/devices/:udn/services/:service/variables
```

**Réponse :**
```json
{
  "udn": "uuid:f9ef6c21-0ed3-470c-9846-bc1ae85fea62",
  "service": "AVTransport",
  "variables": [
    {
      "name": "TransportState",
      "value": "STOPPED",
      "sends_events": true
    },
    {
      "name": "TransportStatus",
      "value": "OK",
      "sends_events": true
    },
    {
      "name": "CurrentTrackURI",
      "value": "",
      "sends_events": true
    }
  ]
}
```

## Développement d'une interface web

L'API REST permet de créer facilement un composant Vue.js pour explorer l'état du serveur UPnP :

```vue
<template>
  <div class="upnp-explorer">
    <h2>UPnP Devices</h2>
    <div v-for="device in devices" :key="device.udn">
      <h3>{{ device.friendly_name }}</h3>
      <button @click="loadDeviceDetails(device.udn)">Details</button>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue';

const devices = ref([]);

onMounted(async () => {
  const response = await fetch('/api/upnp/devices');
  const data = await response.json();
  devices.value = data.devices;
});

async function loadDeviceDetails(udn) {
  const response = await fetch(`/api/upnp/devices/${udn}`);
  const data = await response.json();
  console.log(data);
}
</script>
```

## Structures de données

### DeviceRegistry

Maintient la collection de tous les `DeviceInstance` avec :
- Double indexation (par nom et UDN)
- Méthodes d'introspection
- Modification des variables d'état

### Structures sérialisables

Toutes les structures sont sérialisables en JSON via Serde :
- `DeviceInfo` : Informations complètes sur un device
- `ServiceInfo` : Informations sur un service
- `ActionInfo` : Informations sur une action
- `ArgumentInfo` : Informations sur un argument
- `VariableInfo` : Informations sur une variable d'état

## Fonctions helper

### `upnp_server::with_devices`

Exécute une closure avec accès aux devices :

```rust
use pmoupnp::upnp_server::with_devices;

let device_count = with_devices(|devices| devices.len());
```

### `upnp_server::get_device_by_udn`

Récupère un device par son UDN :

```rust
use pmoupnp::upnp_server::get_device_by_udn;

if let Some(device) = get_device_by_udn("uuid:...") {
    println!("Found: {}", device.get_name());
}
```

## Modification des variables (TODO)

L'API pour modifier les variables sera ajoutée ultérieurement via des endpoints POST/PUT :

```http
PUT /api/upnp/devices/:udn/services/:service/variables/:variable
Content-Type: application/json

{
  "value": "PLAYING"
}
```

## Notes d'implémentation

- Le `DeviceRegistry` est stocké en `thread_local!` pour ne pas modifier `pmoserver::Server`
- Suit le pattern d'extension utilisé dans PMOMusic (traits `UpnpServer` et `UpnpApiExt`)
- Compatible avec l'architecture existante de `pmolog` et `pmocovers`
