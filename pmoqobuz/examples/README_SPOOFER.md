# Exemple Spoofer Qobuz

Cet exemple reproduit le comportement du spoofer Python original pour extraire dynamiquement l'AppID et les secrets de l'API Qobuz.

## Vue d'ensemble

Le spoofer effectue les opérations suivantes :

1. **Récupère la page de login** : `https://play.qobuz.com/login`
2. **Extrait l'URL du bundle.js** : Via regex sur la page HTML
3. **Télécharge le bundle** : JavaScript obfusqué contenant les secrets
4. **Extrait l'AppID** : Via regex spécifique
5. **Extrait les secrets** : Via une série de regex et décodage base64

## Équivalences Python ↔ Rust

| Python | Rust | Notes |
|--------|------|-------|
| `requests.get()` | `reqwest::Client::get()` | Client HTTP asynchrone |
| `re.search()` / `re.finditer()` | `regex::Regex::captures()` / `captures_iter()` | Expressions régulières |
| `OrderedDict` | `indexmap::IndexMap` | Maintient l'ordre d'insertion |
| `base64.standard_b64decode()` | `base64::STANDARD.decode()` | Décodage base64 |
| String slicing `[:-44]` | `&string[..len-44]` | Extraction de sous-chaînes |

## Différences notables

### 1. Gestion asynchrone
Le code Rust est entièrement asynchrone avec Tokio :
```rust
#[tokio::main]
async fn main() -> Result<()> {
    let spoofer = Spoofer::new().await?;
    // ...
}
```

### 2. Gestion d'erreurs explicite
Rust utilise `Result<T, E>` pour la gestion d'erreurs :
```rust
fn get_app_id(&self) -> Result<String> {
    let captures = self.app_id_regex
        .captures(&self.bundle)
        .ok_or_else(|| anyhow::anyhow!("AppID non trouvé"))?;
    // ...
}
```

### 3. Propriété et emprunt
Rust nécessite une gestion explicite de la propriété :
```rust
// Clone pour éviter les problèmes de borrowing
let second_key = keys[1].clone();
let second_value = secrets.get(&second_key).unwrap().clone();
```

### 4. Réorganisation de l'IndexMap
Le code Python utilise `move_to_end()` :
```python
secrets.move_to_end(keypairs[1][0], last=False)
```

En Rust, on reconstruit une nouvelle map :
```rust
secrets.shift_remove(&second_key);
let mut new_secrets = IndexMap::new();
new_secrets.insert(second_key, second_value);
for (k, v) in secrets {
    new_secrets.insert(k, v);
}
```

## Usage

```bash
# Compiler et lancer l'exemple
cargo run --example spoofer

# Ou compiler uniquement
cargo check --example spoofer
```

## Sortie attendue

```
=== Spoofer Qobuz ===

Récupération de la page de login...
Téléchargement du bundle depuis: /resources/x.x.x-xxxx/bundle.js
Bundle téléchargé (xxxxx bytes)
Timezones trouvées: ["america", "europe", "asia", ...]

--- App ID ---
App ID: 123456789

--- Secrets ---
america: xxxxxxxxxxxxxxxxxxxxxxxxx
europe: yyyyyyyyyyyyyyyyyyyyyyyyy
...
```

## Dépendances

Les dépendances suivantes sont nécessaires (ajoutées dans `[dev-dependencies]`) :

```toml
regex = "1.10"
base64 = "0.22"
indexmap = "2.0"
```

## Avertissement

⚠️ **Note importante** : Ce code est fourni à des fins éducatives et de reverse engineering. L'extraction de secrets depuis des applications web peut violer les conditions d'utilisation de certains services. Utilisez-le de manière responsable et conformément aux lois applicables.

## Références

- Code Python original : Basé sur le spoofer Qobuz de la communauté
- Documentation Qobuz API : https://github.com/Qobuz/api-documentation
