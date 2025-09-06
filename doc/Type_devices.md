Voici un tableau complet des devices et services UPnP/DLNA pertinents pour un **serveur audio**, prêt à structurer dans ton code :

| Device type                    | UPnP class                                    | Rôle                             | Services indispensables                                | Description                                                                                  |
| ------------------------------ | --------------------------------------------- | -------------------------------- | ------------------------------------------------------ | -------------------------------------------------------------------------------------------- |
| MediaServer (MS)               | `urn:schemas-upnp-org:device:MediaServer:1`   | Fournit des fichiers audio/vidéo | `ContentDirectory`, `ConnectionManager`                | Permet aux clients de parcourir et lire le contenu stocké sur le serveur                     |
| MediaRenderer (MR)             | `urn:schemas-upnp-org:device:MediaRenderer:1` | Reçoit et lit l’audio            | `RenderingControl`, `AVTransport`, `ConnectionManager` | Lecteur audio DLNA : contrôle de volume, transport (play/pause/stop), gestion des connexions |
| Digital Media Controller (DMC) | Pas de device standard, souvent logiciel      | Orchestration entre MS et MR     | Contrôle MR via `AVTransport` et `RenderingControl`    | Contrôle à distance la lecture et le flux audio sur un ou plusieurs MR                       |
| Playlist Server (optionnel)    | Dépend du vendor                              | Gère playlists                   | Souvent custom                                         | Fournit des listes de lecture pour les MR, parfois intégré au MS                             |
| Remote UI / Control Point      | `urn:schemas-upnp-org:device:ControlPoint:1`  | Interface de contrôle            | Aucun service UPnP standard                            | Applications ou UI qui pilotent la lecture sur MR/MS                                         |

---

### Services clés pour un **MediaRenderer audio**

| Service               | SCPD / Actions principales                           | Description                                                     |
| --------------------- | ---------------------------------------------------- | --------------------------------------------------------------- |
| `RenderingControl:1`  | `SetVolume`, `GetVolume`, `SetMute`, `GetMute`       | Contrôle du volume et mute                                      |
| `AVTransport:1`       | `SetAVTransportURI`, `Play`, `Pause`, `Stop`, `Seek` | Commandes de lecture audio/vidéo                                |
| `ConnectionManager:1` | `GetProtocolInfo`, `PrepareForConnection`            | Informations sur les protocoles supportés et gestion de session |

### Résumé pour un **MediaServer audio minimal**

| Service               | Actions clés                                                    | Notes                                                            |
| --------------------- | --------------------------------------------------------------- | ---------------------------------------------------------------- |
| `ContentDirectory:1`  | `Browse`, `Search`, `GetSystemUpdateID`                         | Obligatoire pour parcourir le contenu audio                      |
| `ConnectionManager:1` | `GetProtocolInfo`, `PrepareForConnection`, `ConnectionComplete` | Obligatoire pour que les MediaRenderers puissent lire le contenu |
| Autres                | optionnels                                                      | Selon fonctionnalités avancées (playlists, enregistrement, etc.) |

---

### Bonnes pratiques

1. **UDN unique et persistant** pour chaque device.
2. **Hierarchie de device** : MediaServer et MediaRenderer peuvent être enfants ou racines selon le schéma Mermaid que tu utilises.
3. **URLs SCPD et services** doivent suivre le schéma :

```
/device/<device-type>/<udn>/desc.xml
/device/<device-type>/<udn>/service/<service>.xml
```

4. Si ton serveur est audio-only, **MediaRenderer est suffisant** pour exposer l’interface de lecture et contrôle, MediaServer si tu fournis du contenu.


Pour un **MediaServer (MS)** UPnP/DLNA, les services standard à implémenter sont bien définis dans les specs **UPnP AV** et **DLNA Guidelines** :

---

### 1. **ContentDirectory:1**

* **Rôle** : exposer le contenu multimédia (audio, vidéo, images).

* **Actions principales** :

  * `Browse` : lister les objets (dossiers/fichiers) dans la bibliothèque.
  * `Search` : rechercher des objets selon des critères.
  * `GetSystemUpdateID` : pour détecter les modifications dans la bibliothèque.
  * `GetSortCapabilities` / `GetSearchCapabilities` : métadonnées supportées.

* **URL SCPD** : `/device/<devicetype>/<udn>/service/ContentDirectory.xml`

---

### 2. **ConnectionManager:1**

* **Rôle** : gérer les connexions et informer sur les protocoles supportés.

* **Actions principales** :

  * `GetProtocolInfo` : retourne les protocoles de lecture supportés (DLNA profile, MIME type).
  * `PrepareForConnection` / `ConnectionComplete` : notification de début/fin de connexion entre MS et MR.
  * `GetCurrentConnectionIDs` / `GetCurrentConnectionInfo` : état des connexions.

* **URL SCPD** : `/device/<devicetype>/<udn>/service/ConnectionManager.xml`

---

### 3. **Optional / Extensions**

Certains MediaServer ajoutent des services supplémentaires :

* `ScheduledRecording` : pour enregistrer du contenu.
* `ImportResource` : pour ajouter dynamiquement des fichiers.
* Services vendor-specific pour métadonnées enrichies ou playlists.
* Chaque **MediaServer doit avoir un UDN persistant**.
* Les services doivent renvoyer des **protocolInfo** corrects pour le DLNA audio (ex : `http-get:*:audio/mpeg:*`).

---
