# Résolution du Problème UPnP - PMO Music MediaServer

**Date:** 2025-11-26
**Problème:** Le serveur UPnP de PMO Music n'est pas reconnu par BubbleUPnP

## Diagnostic

Après une analyse approfondie avec des outils de découverte UPnP et de tests SOAP, le problème identifié était :

**🔴 PROBLÈME CRITIQUE : `SourceProtocolInfo` vide**

Le service `ConnectionManager` du MediaServer retournait des valeurs vides pour `SourceProtocolInfo`, ce qui empêchait les clients UPnP (comme BubbleUPnP) de savoir quels formats audio le serveur pouvait fournir.

### Réponse AVANT la correction :

```xml
<u:GetProtocolInfoResponse>
  <Source></Source>  <!-- ❌ VIDE -->
  <Sink></Sink>
</u:GetProtocolInfoResponse>
```

## Solution Implémentée

### 1. Nouveau Module : `device_ext.rs`

Création d'un trait d'extension `MediaServerDeviceExt` pour `Arc<DeviceInstance>` qui initialise automatiquement les `ProtocolInfo`.

**Fichier:** [`pmomediaserver/src/device_ext.rs`](pmomediaserver/src/device_ext.rs)

```rust
pub trait MediaServerDeviceExt {
    /// Initialise les ProtocolInfo du ConnectionManager pour PMO Music.
    ///
    /// PMO Music convertit tous les flux audio en FLAC (et OGG-FLAC).
    fn init_protocol_info(&self);
}
```

### 2. Formats Supportés

PMO Music convertit tout au vol en FLAC, donc `SourceProtocolInfo` annonce :

- `http-get:*:audio/flac:*` - FLAC standard
- `http-get:*:audio/x-flac:*` - FLAC (format alternatif)
- `http-get:*:application/flac:*` - FLAC (MIME type alternatif)
- `http-get:*:application/x-flac:*` - FLAC (MIME type alternatif)
- `http-get:*:application/ogg:*` - OGG-FLAC
- `http-get:*:audio/ogg:*` - OGG-FLAC
- `http-get:*:audio/x-ogg:*` - OGG-FLAC (format alternatif)

### 3. Intégration dans `main.rs`

**Fichier:** [`PMOMusic/src/main.rs`](PMOMusic/src/main.rs)

```rust
use pmomediaserver::MediaServerDeviceExt;

let server_instance = server
    .write()
    .await
    .register_device(MEDIA_SERVER.clone())
    .await
    .expect("Failed to register MediaServer");

// ✅ Initialiser les ProtocolInfo du MediaServer
server_instance.init_protocol_info();
```

### 4. Export dans `lib.rs`

**Fichier:** [`pmomediaserver/src/lib.rs`](pmomediaserver/src/lib.rs)

```rust
pub mod device_ext;
pub use device_ext::MediaServerDeviceExt;
```

## Réponse APRÈS la correction

```xml
<u:GetProtocolInfoResponse>
  <Source>http-get:*:audio/flac:*,http-get:*:audio/x-flac:*,http-get:*:application/flac:*,http-get:*:application/x-flac:*,http-get:*:application/ogg:*,http-get:*:audio/ogg:*,http-get:*:audio/x-ogg:*</Source>  <!-- ✅ INITIALISÉ -->
  <Sink></Sink>  <!-- ✅ Vide pour un MediaServer (normal) -->
</u:GetProtocolInfoResponse>
```

## Fichiers Modifiés

1. ✅ **Nouveau:** `pmomediaserver/src/device_ext.rs` - Trait d'extension pour initialiser ProtocolInfo
2. ✅ **Modifié:** `pmomediaserver/src/lib.rs` - Export du trait
3. ✅ **Modifié:** `PMOMusic/src/main.rs` - Appel à `init_protocol_info()`

## Test de Validation

Après redémarrage du serveur PMO Music, vérifier avec :

```bash
python3 tools/test_soap.py
```

Ou directement :

```bash
curl -X POST \
  -H "Content-Type: text/xml" \
  -H "SOAPAction: \"urn:schemas-upnp-org:service:ConnectionManager:1#GetProtocolInfo\"" \
  -d '<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <u:GetProtocolInfo xmlns:u="urn:schemas-upnp-org:service:ConnectionManager:1"/>
  </s:Body>
</s:Envelope>' \
  http://localhost:8080/device/.../service/ConnectionManager/control
```

## Prochaines Étapes

1. ✅ Redémarrer le serveur PMO Music
2. ⏳ Tester avec BubbleUPnP pour confirmer que le serveur est maintenant reconnu
3. ⏳ (Optionnel) Ajouter une icône pour le MediaServer (amélioration UX)
4. ⏳ (Optionnel) Passer à specVersion 1.1 (amélioration de compatibilité)

## Références

- Rapport d'analyse complet : [`UPNP_ANALYSIS_REPORT.md`](UPNP_ANALYSIS_REPORT.md)
- UPnP AV Architecture Specification :
  https://openconnectivity.org/developer/specifications/upnp-resources/upnp/
