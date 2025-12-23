# pmoconfig - PMOMusic Configuration Module

Module de gestion de configuration pour PMOMusic avec support du chiffrement des mots de passe.

## Fonctionnalités

- ✅ **Configuration YAML** avec valeurs par défaut intégrées
- ✅ **Fusion automatique** entre config par défaut et config utilisateur
- ✅ **Overrides via variables d'environnement** (`PMOMUSIC_CONFIG__`)
- ✅ **Getters/setters type-safe** pour les valeurs de configuration
- ✅ **Pattern singleton thread-safe** pour l'accès global
- ✅ **🔒 Chiffrement des mots de passe** basé sur l'UUID de la machine
- ✅ **API REST optionnelle** (feature `api`)

## Utilisation de base

```rust
use pmoconfig::get_config;

// Obtenir la configuration globale
let config = get_config();

// Lire des valeurs
let port = config.get_http_port();
let cache_dir = config.get_cover_cache_dir()?;

// Modifier des valeurs
config.set_http_port(9000)?;
```

## Chiffrement des mots de passe

PMOConfig intègre un système de chiffrement transparent des mots de passe basé sur l'UUID matériel de la machine.

### Chiffrer un mot de passe

```bash
cargo run --example encrypt_password -- encrypt "MonMotDePasse"
```

**Sortie** :
```
Original:  MonMotDePasse
Encrypted: encrypted:yRyu/jNlJRSdVz0eE+JX56UC2Tk016TmESDoLT6npLBJB3ZuhJ0XTqNOQjiXkkcB

Add this to your config.yaml:
password: "encrypted:yRyu/jNlJRSdVz0eE+JX56UC2Tk016TmESDoLT6npLBJB3ZuhJ0XTqNOQjiXkkcB"
```

### Configuration

**config.yaml avec mot de passe chiffré** :
```yaml
accounts:
  qobuz:
    username: user@example.com
    password: encrypted:yRyu/jNlJRSdVz0eE+JX56UC2Tk016TmESDoLT6npLBJB3ZuhJ0XTqNOQjiXkkcB
```

### Utilisation dans le code

```rust
use pmoconfig::encryption;

// Déchiffrement automatique (gère plaintext et encrypted)
let password = encryption::get_password(&value)?;

// Chiffrer
let encrypted = encryption::encrypt_password("secret")?;

// Déchiffrer
let decrypted = encryption::decrypt_password(&encrypted)?;

// Tester si chiffré
if encryption::is_encrypted(&value) {
    // ...
}
```

### Caractéristiques du chiffrement

- **Algorithme** : AES-256-GCM
- **Clé** : Dérivée de l'UUID matériel (SHA-256)
- **Format** : `encrypted:BASE64(nonce + ciphertext)`
- **Déterministe** : Même password = même ciphertext

### Avantages

✅ Pas de keyring/keychain requis
✅ Pas de clé maître à gérer
✅ Transparent pour l'utilisateur
✅ Migration progressive (supporte plaintext et encrypted)
✅ Déchiffrement automatique

### Limitations

⚠️ Non portable entre machines
⚠️ Sécurité limitée contre accès physique
⚠️ Pas de protection contre root/admin

📖 **Documentation complète** : [PASSWORD_ENCRYPTION.md](PASSWORD_ENCRYPTION.md)

## Structure de la configuration

```yaml
host:
  http_port: 8080
  base_url: "http://192.168.1.10:8080"
  cover_cache:
    directory: cache_covers
    size: 2000
  audio_cache:
    directory: cache_audio
    size: 500
  logger:
    buffer_capacity: 200
    enable_console: true
    min_level: INFO

playlists:
  directory: playlists

devices:
  mediarenderer:
    pmo_mediarenderer:
      udn: e4b68fbc-2bd5-4cea-98d8-be843fec0bd4
  mediaserver:
    pmo_mediaserver:
      udn: 17fe2ea6-8908-4e30-bc52-b28ea4cab3e4

accounts:
  qobuz:
    username: user@example.com
    password: encrypted:ABC123...  # ← Mot de passe chiffré
    appid: '798273057'
    secret: 806331c3b0b641da923b890aed01d04a
```

## Répertoires de configuration

La configuration est recherchée dans cet ordre :

1. Répertoire fourni en paramètre
2. Variable d'environnement `PMOMUSIC_CONFIG`
3. `.pmomusic` dans le répertoire courant
4. `.pmomusic` dans le répertoire home (`~/.pmomusic`)

## Overrides via variables d'environnement

```bash
# Format: PMOMUSIC_CONFIG__section__key
export PMOMUSIC_CONFIG__host__http_port=9000
export PMOMUSIC_CONFIG__host__logger__min_level=DEBUG

# Lancer l'application
./pmomusic
```

## API REST (feature `api`)

```toml
[dependencies]
pmoconfig = { path = "../pmoconfig", features = ["api"] }
```

```rust
use pmoconfig::api::create_config_router;
use axum::Router;

let config_router = create_config_router();
let app = Router::new().nest("/api/config", config_router);
```

**Endpoints disponibles** :
- `GET /api/config` - Récupère toute la configuration
- `GET /api/config/{path}` - Récupère une valeur spécifique
- `PUT /api/config/{path}` - Modifie une valeur
- `GET /api/config/docs` - Documentation OpenAPI/Swagger

## Exemples

### Exemple complet

Voir [examples/encrypt_password.rs](examples/encrypt_password.rs) pour un exemple complet de chiffrement/déchiffrement.

### Utilisation dans un projet

```rust
use pmoconfig::{get_config, encryption};
use anyhow::Result;

fn main() -> Result<()> {
    let config = get_config();

    // Lire la configuration
    let port = config.get_http_port();
    println!("HTTP port: {}", port);

    // Lire un mot de passe (automatiquement déchiffré)
    let password_value = config.get_value(&["accounts", "service", "password"])?;
    if let serde_yaml::Value::String(s) = password_value {
        let password = encryption::get_password(&s)?;
        println!("Password loaded successfully");
    }

    Ok(())
}
```

## Tests

```bash
# Tests unitaires
cargo test

# Tests du module encryption
cargo test encryption

# Tester l'outil de chiffrement
cargo run --example encrypt_password -- test
```

## Dépendances

```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_yaml = "0.9"
anyhow = "1.0"
dirs = "6.0"
uuid = { version = "1.18", features = ["v4"] }
tracing = "0.1"

# Chiffrement
aes-gcm = "0.10"
sha2 = "0.10"
base64 = "0.22"

# Feature API (optionnel)
axum = { version = "0.8", optional = true }
utoipa = { version = "5.3", optional = true }
```

## Licence

Voir LICENSE dans la racine du projet.
