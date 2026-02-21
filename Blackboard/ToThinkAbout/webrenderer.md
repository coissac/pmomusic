Parfait. Voici un **schéma fonctionnel minimal** pour un **MediaRenderer UPnP privé par navigateur** avec **token**. L’idée est de rester fidèle à ton backend Rust existant et à la webapp Vue.js. En s'appuyant sur l'architecture de PMOMusic, j'aimerais que tu proposes un plan détaillé pour implémenter un tel système de Média Renderer.

- L'application web se trouve dans: [@webapp](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoapp/webapp) 
- Tu as un prototype de Média Renderer dans: [@pmomediarenderer](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmomediarenderer) 
- Le contrôle point est dans : [@pmocontrol](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmocontrol) 
- Tu implémenteras ce nouveau système de Média Renderer dans la CRATe pmowebrenderer

Tu mettras une version du plan en Markdown dans le répertoire [@Architecture](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Architecture) .

---

## 1. Flow général

```
Browser (Vue.js Control Point)
 ┌───────────────┐
 │   UI / audio  │
 │  WebSocket    │
 └───────▲───────┘
         │ token
         │
         ▼
Rust backend (UPnP MediaRenderer)
 ┌───────────────────────────┐
 │ Token → Renderer mapping  │
 │ Device XML / SOAP endpoints│
 │ Play/Pause/Stop → WS → Browser │
 └───────────────────────────┘
```

---

## 2. Étapes détaillées

### a) Création du renderer

1. Le navigateur se connecte via WebSocket ou HTTP.
2. Rust génère un token unique pour ce client :

   ```rust
   use uuid::Uuid;
   let token = Uuid::new_v4().to_string();
   ```
3. Rust crée une instance MediaRenderer **privée**, associée à ce token :

   * Device description XML : `/renderer/<token>/desc.xml`
   * AVTransport SOAP : `/renderer/<token>/avtransport`
   * RenderingControl SOAP : `/renderer/<token>/renderingcontrol`

---

### b) Control Point

* La webapp Vue.js reçoit le token et la “déclare” au Control Point :

```js
const renderer = {
  token: "abcd-1234-efgh",
  name: "Browser Renderer"
};

// Ajout au control point local
controlPoint.addRenderer(renderer);
```

* Toutes les commandes Play/Pause/Stop incluent ce token :

```js
ws.send(JSON.stringify({
  token: renderer.token,
  action: "play",
  uri: "http://localhost:8080/media.mp3"
}));
```

---

### c) Backend Rust : dispatcher les commandes

* Rust reçoit le JSON avec le token.
* Vérifie que le token correspond à un renderer actif.
* Transmet la commande au navigateur via WebSocket (ou HTTP push) :

```rust
match msg.action.as_str() {
    "play" => send_ws_to_browser(&token, format!("play:{}", msg.uri)),
    "pause" => send_ws_to_browser(&token, "pause".to_string()),
    "stop" => send_ws_to_browser(&token, "stop".to_string()),
    _ => (),
}
```

* Rust met à jour l’état du renderer (AVTransport/RenderingControl) pour le Control Point.

---

### d) Lecture côté navigateur

* Le navigateur reçoit la commande via WebSocket et pilote `<audio>` :

```js
ws.onmessage = (evt) => {
  const msg = evt.data;
  if(msg.startsWith("play:")) {
    audio.src = msg.split(":")[1];
    audio.play();
  } else if(msg === "pause") {
    audio.pause();
  } else if(msg === "stop") {
    audio.pause();
    audio.currentTime = 0;
  }
};
```

---

### e) Fermeture / cleanup

* Quand le navigateur se déconnecte :

  * Rust supprime le renderer associé au token
  * Émet un **byebye virtuel** pour le Control Point (si nécessaire)
  * Libère toutes les ressources

---

## 3. Points clés

1. **Token unique** = session privée + sécurité
2. **Pas besoin de SSDP / annonce** : le renderer est dédié à un navigateur connu
3. **Control Point Vue.js** sait exactement quel renderer utiliser
4. **Rust backend** reste seul responsable de l’implémentation UPnP
5. **Lecture réelle** = navigateur via `<audio>` ou `<video>`

---

💡 Ce modèle est très proche de ce que font **Mopidy avec Iris**, **Kodi Remote**, ou **Chromecast / local cast** : le device est connu et dédié, pas besoin de découverte réseau.
