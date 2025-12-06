# Rapport d'Analyse UPnP - PMO Music vs Serveurs Fonctionnels

**Date:** 2025-11-26
**Problème:** Le serveur UPnP de PMO Music n'est pas reconnu par BubbleUPnP

## Résumé Exécutif

Le serveur PMO Music MediaServer est correctement découvert via SSDP et répond aux requêtes SOAP, mais présente plusieurs différences avec les serveurs qui fonctionnent (comme Upmpdcli). Les problèmes identifiés sont principalement liés aux en-têtes HTTP et aux métadonnées du device.

## Découverte Réseau

### Devices UPnP Détectés

| Device | IP | USN | Status |
|--------|------|-----|---------|
| PMO Music MediaServer | 192.168.0.138:8080 | uuid:8b8e9b19-9c65-4d59-b127-b34717658085 | ✅ Découvert |
| Upmpdcli (pizzicato) | 192.168.0.200:49152 | uuid:c110358f-d885-b44a-d6d3-dca6329ead0d | ✅ Découvert |
| Freebox | 192.168.0.254:52424 | uuid:e929a46e-d218-377d-2dde-32bd8080dfbf | ✅ Découvert |
| Jellyfin | 192.168.0.34:8096 | uuid:526dedec-fde2-4224-bac6-06f7b11711cf | ✅ Découvert |

**Conclusion SSDP:** ✅ PMO Music est correctement annoncé et découvert via SSDP

## Comparaison des Descripteurs XML

### PMO Music MediaServer

```xml
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion>
    <major>1</major>
    <minor>0</minor>  <!-- ⚠️ Version 1.0 -->
  </specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaServer:1</deviceType>
    <friendlyName>PMOMusic Media Server</friendlyName>
    <manufacturer>PMOMusic</manufacturer>
    <modelName>PMOMusic Media Server</modelName>
    <UDN>uuid:8b8e9b19-9c65-4d59-b127-b34717658085</UDN>  <!-- ✅ Format correct -->
    <!-- ❌ Pas d'iconList -->
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:ContentDirectory:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:ContentDirectory</serviceId>
        <SCPDURL>/device/.../service/ContentDirectory/desc.xml</SCPDURL>
        <controlURL>/device/.../service/ContentDirectory/control</controlURL>
        <eventSubURL>/device/.../service/ContentDirectory/event</eventSubURL>
      </service>
      <service>
        <serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType>
        ...
      </service>
    </serviceList>
  </device>
</root>
```

### Upmpdcli (Fonctionnel)

```xml
<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion>
    <major>1</major>
    <minor>1</minor>  <!-- ✅ Version 1.1 -->
  </specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaServer:1</deviceType>
    <manufacturer>lesbonscomptes.com/upmpdcli</manufacturer>
    <modelName>Upmpdcli Media Server</modelName>
    <friendlyName>pizzicato-Music-mediaserver</friendlyName>
    <iconList>  <!-- ✅ Présence d'icônes -->
      <icon>
        <mimetype>image/png</mimetype>
        <width>64</width>
        <height>64</height>
        <depth>32</depth>
        <url>/uuid-.../icon.png</url>
      </icon>
    </iconList>
    <UDN>uuid:c110358f-d885-b44a-d6d3-dca6329ead0d</UDN>
    <serviceList>
      <!-- Mêmes services -->
    </serviceList>
  </device>
</root>
```

### Différences Clés dans le Descripteur

| Élément | PMO Music | Upmpdcli | Impact |
|---------|-----------|----------|---------|
| **specVersion minor** | 0 | 1 | ⚠️ Moyen - Certains clients peuvent filtrer par version |
| **Ordre des éléments** | deviceType, friendlyName, manufacturer, modelName, UDN | deviceType, manufacturer, modelName, friendlyName, iconList, UDN | ⚠️ Faible - Ordre différent mais valide XML |
| **iconList** | ❌ Absent | ✅ Présent | ⚠️ Moyen - Requis pour certains clients |
| **UDN prefix** | ✅ uuid: | ✅ uuid: | ✅ Correct |

## Comparaison des Réponses SOAP

### Test 1: ConnectionManager::GetProtocolInfo

#### PMO Music
```http
Status: 200 OK
Content-Type: (absent)  ⚠️ PROBLÈME CRITIQUE

<?xml version="1.0" encoding="UTF-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"
            s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:GetProtocolInfoResponse xmlns:u="urn:schemas-upnp-org:service:ConnectionManager:1">
      <Source></Source>  ⚠️ Vide
      <Sink></Sink>      ⚠️ Vide
    </u:GetProtocolInfoResponse>
  </s:Body>
</s:Envelope>
```

#### Upmpdcli
```http
Status: 200 OK
Content-Type: text/xml; charset="utf-8"  ✅ Présent

<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"
            s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:GetProtocolInfoResponse xmlns:u="urn:schemas-upnp-org:service:ConnectionManager:1">
      <Source></Source>
      <Sink>http-get:*:audio/flac:*,http-get:*:audio/mp3:*,...</Sink>  ✅ Formats listés
    </u:GetProtocolInfoResponse>
  </s:Body>
</s:Envelope>
```

### Test 2: ContentDirectory::Browse

Les deux serveurs répondent correctement, mais PMO Music manque toujours le header `Content-Type`.

## Problèmes Identifiés par Ordre de Criticité

### 🔴 CRITIQUE

1. **Absence du header Content-Type dans les réponses SOAP**
   - **Impact:** Les clients UPnP stricts (comme BubbleUPnP) peuvent rejeter les réponses sans Content-Type
   - **Spec UPnP:** La spécification UPnP Device Architecture 1.0 exige `Content-Type: text/xml; charset="utf-8"`
   - **Localisation probable:** Dans le code de réponse SOAP du serveur UPnP
   - **Fichiers à vérifier:**
     - `pmoupnp/src/services/service_instance.rs` (handler SOAP)
     - `pmoupnp/src/soap/builder.rs`

2. **ProtocolInfo vide pour Source et Sink**
   - **Impact:** Les clients ne savent pas quels formats audio sont supportés
   - **Spec UPnP:** ConnectionManager doit annoncer les formats supportés
   - **Action:** Implémenter la liste des formats dans ConnectionManager

### 🟡 MOYEN

3. **specVersion 1.0 au lieu de 1.1**
   - **Impact:** Certains clients modernes peuvent filtrer les devices UPnP 1.0
   - **Solution:** Passer à specVersion 1.1

4. **Absence d'iconList**
   - **Impact:** Pas d'icône visible dans les clients UPnP
   - **Solution:** Ajouter au moins une icône PNG 64x64

### 🟢 FAIBLE

5. **Ordre des éléments XML différent**
   - **Impact:** Minimal - XML valide dans tous les cas
   - **Action:** Optionnel - standardiser l'ordre

## Recommandations d'Implémentation

### Priorité 1: Corriger le Content-Type

Localiser le code qui génère les réponses SOAP et ajouter le header:

```rust
// Dans pmoupnp/src/services/service_instance.rs ou similaire
(
    StatusCode::OK,
    [(header::CONTENT_TYPE, "text/xml; charset=\"utf-8\"")],  // ← AJOUTER
    xml
)
```

### Priorité 2: Implémenter GetProtocolInfo correctement

Dans ConnectionManager, retourner la liste des formats supportés:

```rust
// Exemple de formats à supporter
let sink_protocols = vec![
    "http-get:*:audio/flac:*",
    "http-get:*:audio/mpeg:*",
    "http-get:*:audio/mp4:*",
    "http-get:*:audio/ogg:*",
    // ...
];
```

### Priorité 3: Passer à UPnP 1.1

Changer la specVersion de 1.0 à 1.1 dans le device descriptor.

### Priorité 4: Ajouter une icône

Créer une icône PNG 64x64 et l'ajouter au descripteur:

```xml
<iconList>
  <icon>
    <mimetype>image/png</mimetype>
    <width>64</width>
    <height>64</height>
    <depth>32</depth>
    <url>/icon.png</url>
  </icon>
</iconList>
```

## Fichiers à Modifier

1. **pmoupnp/src/services/service_instance.rs** - Ajouter Content-Type aux réponses SOAP
2. **pmoupnp/src/devices/device_methods.rs** - Ajouter iconList au descripteur
3. **pmoupnp/src/devices/device.rs** - Passer specVersion à 1.1
4. **pmomediaserver/src/connectionmanager/actions/getprotocolinfo.rs** - Implémenter la liste des formats

## Tests de Validation

Après les corrections, vérifier:

1. ✅ `curl` sur le descripteur montre specVersion 1.1 et iconList
2. ✅ Requête SOAP GetProtocolInfo retourne `Content-Type: text/xml`
3. ✅ GetProtocolInfo retourne les formats supportés dans Sink
4. ✅ BubbleUPnP détecte et affiche le serveur PMO Music

## Conclusion

Le serveur PMO Music est **fonctionnellement correct** au niveau de SSDP et des services SOAP, mais présente des problèmes de conformité aux standards UPnP qui peuvent causer des rejets par certains clients stricts comme BubbleUPnP.

Les corrections sont simples et localisées. La priorité absolue est d'ajouter le header `Content-Type` aux réponses SOAP.
