# Guide de comparaison Python vs Rust

## Objectif

Comparer exactement ce que Python envoie vs ce que Rust envoie lors de l'appel à `track/getFileUrl`.

## Étape 1: Tester Python avec le fake server

### Terminal 1 - Démarrer le fake server:

```bash
cd pmoqobuz/test_python_qobuz
python3 fake_qobuz_server.py
```

Laissez ce terminal ouvert. Il affichera tous les détails des requêtes.

### Terminal 2 - Tester Python:

```bash
cd pmoqobuz/test_python_qobuz

# Patcher pour utiliser localhost
python3 patch_for_fake.py

# Lancer le test simplifié
python3 test_getfileurl.py
```

**Entrer vos credentials:**
- Username: `eric@coissac.eu` (ou Entrée)
- Password: votre mot de passe

### Terminal 1 - OBSERVER:

Le serveur affichera 2 requêtes:

1. **POST /api.json/0.2/user/login** - Login (pas de signature)
2. **POST /api.json/0.2/track/getFileUrl** - ⚠️ C'EST CELLE-CI QU'ON VEUT!

Pour la requête `track/getFileUrl`, notez EXACTEMENT:

```
Headers:
  X-App-Id: ???
  X-User-Auth-Token: ???
  Content-Type: ???
  Content-Length: ???

Body (Form Data):
  track_id: ???
  format_id: ???
  intent: ???
  request_ts: ???  ← FORMAT DU TIMESTAMP
  request_sig: ??? ← SIGNATURE MD5
```

**IMPORTANT:** Copiez/collez ou prenez une capture d'écran de cette requête!

### Terminal 2 - Restaurer:

```bash
python3 patch_for_fake.py restore
```

---

## Étape 2: Tester Rust avec le fake server

### Terminal 1 - Fake server toujours actif

### Terminal 2 - Modifier temporairement le code Rust:

Éditer `pmoqobuz/src/api/mod.rs` ligne ~32:

```rust
// AVANT:
const API_BASE_URL: &str = "https://www.qobuz.com/api.json/0.2";

// APRÈS (temporaire!):
const API_BASE_URL: &str = "http://localhost:8080/api.json/0.2";
```

### Terminal 2 - Compiler et lancer:

```bash
cd pmoqobuz
cargo build --example lazy_loading
cargo run --example lazy_loading
```

### Terminal 1 - OBSERVER:

Cherchez la requête **POST /api.json/0.2/track/getFileUrl**

Notez les mêmes détails que pour Python:

```
Headers:
  X-App-Id: ???
  X-User-Auth-Token: ???
  Content-Type: ???
  Content-Length: ???

Body (Form Data):
  track_id: ???
  format_id: ???
  intent: ???
  request_ts: ???  ← FORMAT DU TIMESTAMP
  request_sig: ??? ← SIGNATURE MD5
```

### Terminal 2 - Restaurer le code Rust:

Remettre l'URL originale:

```rust
const API_BASE_URL: &str = "https://www.qobuz.com/api.json/0.2";
```

---

## Étape 3: Comparer

Placez les deux requêtes côte à côte et cherchez les différences:

### Différences possibles:

1. **Ordre des paramètres** dans le form data
2. **Format du timestamp** (nombre de décimales?)
3. **Signature MD5** (devrait être différente car timestamp différent)
4. **Headers manquants** (Content-Type?)
5. **Valeurs des headers** (X-App-Id différent?)
6. **Encoding** du form data

### Ce qu'on DOIT voir identique:

- Méthode: POST (pas GET)
- Headers X-App-Id et X-User-Auth-Token présents
- Paramètres: track_id, format_id, intent, request_ts, request_sig
- Content-Type: application/x-www-form-urlencoded

### Ce qui PEUT être différent:

- Valeur de request_ts (timestamp différent)
- Valeur de request_sig (car timestamp différent)
- Ordre des paramètres (si ça n'affecte pas la signature)

### Ce qui NE DOIT PAS être différent:

- **Format** du timestamp (même nombre de décimales)
- Type de requête (POST)
- Présence de tous les headers requis

---

## Exemple de comparaison

### Python (référence):
```
POST /api.json/0.2/track/getFileUrl

Headers:
  X-App-Id: 798273057
  X-User-Auth-Token: NAP_hlSUqU...
  Content-Type: application/x-www-form-urlencoded
  Content-Length: 156

Body:
  track_id=19557883&format_id=27&intent=stream&request_ts=1734170123.456789&request_sig=abc123def456...
```

### Rust (à corriger):
```
POST /api.json/0.2/track/getFileUrl

Headers:
  X-App-Id: 798273057
  X-User-Auth-Token: NAP_hlSUqU...
  Content-Type: application/x-www-form-urlencoded
  Content-Length: 156

Body:
  track_id=19557883&format_id=27&intent=stream&request_ts=1734170123.45678&request_sig=xyz789abc012...
                                                                        ^
                                                                  Une décimale en moins?
```

---

## Notes

- Le fake server log TOUT, donc vous verrez aussi les requêtes de login
- Concentrez-vous sur la requête `/track/getFileUrl` qui nécessite une signature
- Prenez des captures d'écran ou copiez les logs pour comparaison facile
