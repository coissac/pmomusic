# Instructions de test - Comparaison Python vs Rust

## Étape 1: Tester le script Python contre l'API réelle

```bash
cd pmoqobuz/test_python_qobuz
python3 test_qobuz.py
```

**Ce que vous devez entrer:**
- Username: `eric@coissac.eu` (ou appuyez Entrée pour défaut)
- Password: votre mot de passe Qobuz

**Résultat attendu:**
Si le script Python **réussit**, cela confirme que:
- ✅ La méthode de signature Python est correcte
- ✅ Les secrets extraits par le spoofer fonctionnent
- ✅ Notre implémentation Rust a un problème spécifique

Si le script Python **échoue aussi**, cela signifie:
- ❌ Le problème est dans les secrets eux-mêmes
- ❌ Ou Qobuz a changé leur API récemment

---

## Étape 2: Analyser les requêtes avec le fake server

### Terminal 1 - Démarrer le fake server:

```bash
cd pmoqobuz/test_python_qobuz
python3 fake_qobuz_server.py
```

Le serveur affichera tous les détails des requêtes reçues.

### Terminal 2 - Tester avec Python:

```bash
cd pmoqobuz/test_python_qobuz
python3 patch_for_fake.py                    # Redirige vers localhost
python3 test_qobuz.py                         # Entrer credentials
```

**Observer dans Terminal 1:**
- Méthode HTTP (GET/POST) pour `/track/getFileUrl`
- Headers exactes (X-App-Id, X-User-Auth-Token)
- Paramètres et leur ordre
- Format du `request_ts` (timestamp)
- Format du `request_sig` (signature MD5)
- Content-Type du body

### Terminal 2 - Restaurer:

```bash
python3 patch_for_fake.py restore
```

---

## Étape 3: Tester Rust contre le fake server

### Modifier temporairement le code Rust:

Éditer `pmoqobuz/src/api/mod.rs` ligne ~32:

```rust
// Avant:
const API_BASE_URL: &str = "https://www.qobuz.com/api.json/0.2";

// Après (temporaire!):
const API_BASE_URL: &str = "http://localhost:8080/api.json/0.2";
```

### Terminal 1 - Fake server toujours actif

### Terminal 2 - Lancer l'exemple Rust:

```bash
cd pmoqobuz
cargo run --example lazy_loading
```

**Observer dans Terminal 1:**
Les mêmes détails que pour Python.

### Restaurer le code Rust:

```rust
const API_BASE_URL: &str = "https://www.qobuz.com/api.json/0.2";
```

---

## Étape 4: Comparer

Comparez côte à côte:

### Python (référence qui marche):
```
POST /api.json/0.2/track/getFileUrl
Headers:
  X-App-Id: 798273057
  X-User-Auth-Token: NAP_...
  Content-Type: application/x-www-form-urlencoded

Body:
  track_id=12345678
  format_id=27
  intent=stream
  request_ts=1734169183.123456
  request_sig=a1b2c3d4...
```

### Rust (à corriger):
```
POST /api.json/0.2/track/getFileUrl
Headers:
  X-App-Id: ???
  X-User-Auth-Token: ???
  Content-Type: ???

Body:
  ??? ordre différent ???
  ??? timestamp différent ???
  ??? signature différente ???
```

### Différences à chercher:

1. **Ordre des paramètres** - l'ordre affecte-t-il la signature?
2. **Format du timestamp** - nombre de décimales?
3. **Headers manquants** - Content-Type?
4. **Signature MD5** - différente malgré mêmes inputs?
5. **Method** - vraiment POST des deux côtés?

---

## Script automatique

Pour simplifier, vous pouvez aussi utiliser:

```bash
./run_comparison.sh          # Test contre API réelle
./run_comparison.sh fake     # Test avec fake server (interactif)
```

---

## Nettoyage

```bash
# Arrêter tous les fake servers
pkill -f fake_qobuz_server

# Restaurer raw.py si nécessaire
python3 patch_for_fake.py restore
```
