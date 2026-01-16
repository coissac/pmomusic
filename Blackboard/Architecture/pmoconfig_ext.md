# Pattern d'extension de pmoconfig::Config

## Vue d'ensemble

Ce document décrit le pattern architectural utilisé dans PMOMusic pour étendre la configuration centralisée (`pmoconfig::Config`) avec des fonctionnalités spécifiques à chaque crate.

### Objectif

Permettre à chaque crate du projet d'ajouter ses propres méthodes de configuration sans modifier directement `pmoconfig`, tout en maintenant une interface cohérente et type-safe.

### Principe

Chaque crate qui nécessite un accès à la configuration implémente un **trait d'extension** pour `pmoconfig::Config`. Ce trait définit des méthodes helpers spécifiques au domaine du crate (cache, authentification, UPnP, etc.).

## Architecture du pattern

### Structure de base

```
pmoconfig/              # Crate de configuration centralisée
  ├── Config            # Struct principale avec get_value/set_value génériques
  └── encryption        # Module de chiffrement des mots de passe

pmocrate/               # Crate spécialisé (audio, qobuz, upnp, etc.)
  └── config_ext.rs     # Trait d'extension pour Config
      ├── DEFAULT_*     # Constantes pour valeurs par défaut
      ├── XxxConfigExt  # Trait d'extension
      └── impl          # Implémentation du trait pour Config
```

### Flux de données

```
Application
    ↓
Trait d'extension spécialisé (QobuzConfigExt, CacheConfigExt, etc.)
    ↓
pmoconfig::Config (get_value/set_value génériques)
    ↓
Fichier config.yaml
```

## Implémentation d'un trait d'extension

### 1. Structure du fichier config_ext.rs

```rust
//! Extension pour intégrer [fonctionnalité] dans pmoconfig
//!
//! Ce module fournit le trait `XxxConfigExt` qui permet d'ajouter facilement
//! des méthodes de gestion de [fonctionnalité] à pmoconfig::Config.

use anyhow::Result;
use pmoconfig::Config;
use serde_yaml::Value;

// Constantes pour valeurs par défaut
const DEFAULT_XXX_DIR: &str = "cache_xxx";
const DEFAULT_XXX_SIZE: usize = 1000;

/// Trait d'extension pour gérer [fonctionnalité] dans pmoconfig
///
/// Ce trait étend `pmoconfig::Config` avec des méthodes spécifiques
/// à la gestion de [fonctionnalité].
///
/// # Exemple
///
/// ```rust,ignore
/// use pmoconfig::get_config;
/// use pmoxxx::XxxConfigExt;
///
/// let config = get_config();
/// let value = config.get_xxx_value()?;
/// ```
pub trait XxxConfigExt {
    /// Documentation de la méthode getter
    fn get_xxx_value(&self) -> Result<Type>;
    
    /// Documentation de la méthode setter
    fn set_xxx_value(&self, value: Type) -> Result<()>;
}

impl XxxConfigExt for Config {
    fn get_xxx_value(&self) -> Result<Type> {
        // Implémentation
    }
    
    fn set_xxx_value(&self, value: Type) -> Result<()> {
        // Implémentation
    }
}
```

### 2. Patterns de chemins YAML

Les chemins dans la configuration suivent une hiérarchie logique :

#### Configuration hôte/système
```rust
// Chemins sous "host"
&["host", "cache_type", "directory"]  // Répertoires de cache
&["host", "cache_type", "size"]       // Tailles de cache
&["host", "upnp", "manufacturer"]     // Configuration UPnP
```

#### Configuration comptes/services
```rust
// Chemins sous "accounts"
&["accounts", "service", "username"]
&["accounts", "service", "password"]
&["accounts", "service", "auth_token"]
```

#### Configuration sources
```rust
// Chemins sous "sources"
&["sources", "source_name", "enabled"]
&["sources", "source_name", "default_channel"]
```

### 3. Patterns de getters

#### Getter simple avec valeur par défaut

```rust
fn get_xxx_value(&self) -> Result<Type> {
    match self.get_value(&["path", "to", "value"]) {
        Ok(Value::Type(v)) => Ok(v),
        _ => Ok(DEFAULT_VALUE),
    }
}
```

#### Getter avec auto-persistence

Pour les valeurs qui doivent être visibles dans le fichier YAML, le getter persiste automatiquement la valeur par défaut :

```rust
fn get_xxx_enabled(&self) -> Result<bool> {
    match self.get_value(&["path", "to", "enabled"]) {
        Ok(Value::Bool(b)) => Ok(b),
        _ => {
            // Auto-persist la valeur par défaut
            self.set_xxx_enabled(true)?;
            Ok(true)
        }
    }
}
```

**Avantage** : L'utilisateur voit la configuration effective dans le YAML et peut la modifier facilement.

#### Getter optionnel

Pour les valeurs vraiment optionnelles (pas de défaut significatif) :

```rust
fn get_xxx_optional(&self) -> Result<Option<String>> {
    match self.get_value(&["path", "to", "optional"]) {
        Ok(Value::String(s)) if !s.is_empty() => Ok(Some(s)),
        Ok(Value::String(_)) => Ok(None), // String vide
        Ok(_) => Ok(None),                // Mauvais type
        Err(_) => Ok(None),               // Non configuré
    }
}
```

#### Getter avec traitement spécial

##### Déchiffrement de mots de passe

```rust
fn get_xxx_password(&self) -> Result<String> {
    match self.get_value(&["accounts", "xxx", "password"])? {
        Value::String(s) => {
            // Déchiffrement automatique si chiffré
            pmoconfig::encryption::get_password(&s)
                .map_err(|e| anyhow!("Failed to decrypt password: {}", e))
        }
        _ => Err(anyhow!("Password not configured")),
    }
}
```

##### Parsing avec fallback

```rust
fn get_xxx_enum_value(&self) -> Result<EnumType> {
    match self.get_value(&["path", "to", "value"]) {
        Ok(Value::String(s)) => {
            match s.parse::<EnumType>() {
                Ok(kind) => Ok(kind),
                Err(_) => {
                    // Valeur invalide, utiliser et persister le défaut
                    self.set_xxx_enum_value(DEFAULT_ENUM)?;
                    Ok(DEFAULT_ENUM)
                }
            }
        }
        Ok(Value::Number(n)) => {
            // Accepter aussi les valeurs numériques
            if let Some(id) = n.as_u64() {
                EnumType::from_id(id as u8)
            } else {
                self.set_xxx_enum_value(DEFAULT_ENUM)?;
                Ok(DEFAULT_ENUM)
            }
        }
        _ => {
            // Non configuré, persister le défaut
            self.set_xxx_enum_value(DEFAULT_ENUM)?;
            Ok(DEFAULT_ENUM)
        }
    }
}
```

### 4. Patterns de setters

#### Setter simple

```rust
fn set_xxx_value(&self, value: Type) -> Result<()> {
    self.set_value(
        &["path", "to", "value"],
        Value::Type(value.into())
    )
}
```

#### Setter avec transformation

```rust
fn set_xxx_enum(&self, variant: EnumVariant) -> Result<()> {
    // Stocker sous forme conviviale (string) plutôt que numérique
    let name = variant.as_str();
    self.set_value(
        &["path", "to", "enum"],
        Value::String(name.to_string())
    )
}
```

#### Setter multiple (transaction)

```rust
fn set_xxx_auth_info(
    &self,
    token: &str,
    user_id: &str,
    expires_at: u64,
) -> Result<()> {
    // Grouper les modifications liées
    self.set_value(
        &["accounts", "xxx", "auth_token"],
        Value::String(token.to_string())
    )?;
    self.set_value(
        &["accounts", "xxx", "user_id"],
        Value::String(user_id.to_string())
    )?;
    self.set_value(
        &["accounts", "xxx", "token_expires_at"],
        Value::Number(serde_yaml::Number::from(expires_at))
    )?;
    Ok(())
}
```

#### Setter de nettoyage

```rust
fn clear_xxx_info(&self) -> Result<()> {
    // Ne pas propager les erreurs (valeurs peuvent ne pas exister)
    let _ = self.set_value(&["path", "to", "field1"], Value::String(String::new()));
    let _ = self.set_value(&["path", "to", "field2"], Value::Number(Number::from(0)));
    Ok(())
}
```

### 5. Helpers de haut niveau

#### Helper de validation

```rust
fn is_xxx_valid(&self) -> bool {
    // Vérifier plusieurs conditions sans Result
    let has_token = self
        .get_xxx_token()
        .ok()
        .flatten()
        .map(|t| !t.is_empty())
        .unwrap_or(false);
    
    let has_user = self
        .get_xxx_user()
        .ok()
        .flatten()
        .map(|u| !u.is_empty())
        .unwrap_or(false);
    
    has_token && has_user
}
```

#### Factory method

Pour les crates qui fournissent des objets complexes configurables :

```rust
fn create_xxx_cache(&self) -> Result<Arc<Cache>> {
    let dir = self.get_xxx_dir()?;
    let size = self.get_xxx_size()?;
    Ok(Arc::new(crate::new_cache(&dir, size)?))
}

fn create_xxx_client(&self) -> Result<XxxClient> {
    let (username, password) = self.get_xxx_credentials()?;
    XxxClient::builder()
        .credentials(username, password)
        .cache_dir(self.get_xxx_cache_dir()?)
        .build()
}
```

#### Getter combiné

```rust
fn get_xxx_credentials(&self) -> Result<(String, String)> {
    let username = self.get_xxx_username()?;
    let password = self.get_xxx_password()?;
    Ok((username, password))
}
```

### 6. Utilisation des méthodes pmoconfig

#### Répertoires managés

Pour les répertoires qui doivent être créés automatiquement :

```rust
fn get_xxx_cache_dir(&self) -> Result<String> {
    // get_managed_dir crée le répertoire s'il n'existe pas
    self.get_managed_dir(&["host", "xxx_cache", "directory"], "cache_xxx")
}

fn set_xxx_cache_dir(&self, directory: String) -> Result<()> {
    self.set_managed_dir(&["host", "xxx_cache", "directory"], directory)
}
```

#### Méthodes génériques de Config utilisables

```rust
// Lecture de valeur générique
pub fn get_value(&self, path: &[&str]) -> Result<Value>

// Écriture de valeur générique
pub fn set_value(&self, path: &[&str], value: Value) -> Result<()>

// Répertoires managés (création auto)
pub fn get_managed_dir(&self, path: &[&str], default: &str) -> Result<String>
pub fn set_managed_dir(&self, path: &[&str], directory: String) -> Result<()>

// Déchiffrement de mots de passe
pub mod encryption {
    pub fn encrypt_password(password: &str) -> Result<String>
    pub fn decrypt_password(encrypted: &str) -> Result<String>
    pub fn get_password(value: &str) -> Result<String>  // Auto-détection
    pub fn is_encrypted(value: &str) -> bool
}
```

## Patterns spécialisés

### Pattern cache (pmocache)

Le crate `pmocache` fournit un trait générique `CacheConfigExt` que les autres crates de cache peuvent utiliser :

```rust
use pmocache::CacheConfigExt;

impl AudioCacheConfigExt for Config {
    fn get_audiocache_dir(&self) -> Result<String> {
        self.get_cache_dir("audio_cache", DEFAULT_AUDIO_CACHE_DIR)
    }
    
    fn create_audio_cache(&self) -> Result<Arc<Cache>> {
        let dir = self.get_audiocache_dir()?;
        let size = self.get_audiocache_size()?;
        Ok(Arc::new(crate::cache::new_cache(&dir, size)?))
    }
}
```

**Avantage** : Cohérence entre tous les caches (audio, covers, qobuz, etc.)

### Pattern authentification (pmoqobuz)

Pour les services nécessitant une authentification :

```rust
pub trait QobuzConfigExt {
    // Credentials de base
    fn get_qobuz_username(&self) -> Result<String>;
    fn get_qobuz_password(&self) -> Result<String>;  // Auto-decrypt
    fn get_qobuz_credentials(&self) -> Result<(String, String)>;
    
    // Tokens d'authentification
    fn get_qobuz_auth_token(&self) -> Result<Option<String>>;
    fn get_qobuz_user_id(&self) -> Result<Option<String>>;
    fn get_qobuz_token_expires_at(&self) -> Result<Option<u64>>;
    
    // Gestion d'authentification groupée
    fn set_qobuz_auth_info(
        &self, 
        token: &str, 
        user_id: &str, 
        expires_at: u64
    ) -> Result<()>;
    fn clear_qobuz_auth_info(&self) -> Result<()>;
    
    // Validation
    fn is_qobuz_auth_valid(&self) -> bool;
}
```

### Pattern rate limiting (pmoqobuz)

Pour les services avec rate limiting :

```rust
pub trait QobuzConfigExt {
    fn get_qobuz_rate_limit_max_concurrent(&self) -> Result<Option<usize>>;
    fn set_qobuz_rate_limit_max_concurrent(&self, max: usize) -> Result<()>;
    
    fn get_qobuz_rate_limit_min_delay_ms(&self) -> Result<Option<u64>>;
    fn set_qobuz_rate_limit_min_delay_ms(&self, delay_ms: u64) -> Result<()>;
    
    fn is_qobuz_rate_limiting_enabled(&self) -> bool;
    fn set_qobuz_rate_limiting_enabled(&self, enabled: bool) -> Result<()>;
}
```

### Pattern configuration minimale (pmoparadise)

Pour les sources qui nécessitent peu de configuration :

```rust
pub trait RadioParadiseConfigExt {
    // Juste enable/disable
    fn get_paradise_enabled(&self) -> Result<bool>;
    fn set_paradise_enabled(&self, enabled: bool) -> Result<()>;
    
    // Configuration minimale avec valeurs intelligentes par défaut
    fn get_paradise_default_channel(&self) -> Result<u8>;
    fn set_paradise_default_channel(&self, channel: u8) -> Result<()>;
}
```

**Philosophie** : Ne configurer que ce qui doit vraiment l'être. Éviter la sur-configuration.

### Pattern UPnP (pmoupnp)

Pour la configuration des devices UPnP :

```rust
pub trait UpnpConfigExt {
    fn get_upnp_manufacturer(&self) -> Result<String>;
    fn set_upnp_manufacturer(&self, manufacturer: String) -> Result<()>;
    
    fn get_upnp_udn_prefix(&self) -> Result<String>;
    fn set_upnp_udn_prefix(&self, prefix: String) -> Result<()>;
    
    fn get_upnp_model_name_prefix(&self) -> Result<String>;
    fn set_upnp_model_name_prefix(&self, prefix: String) -> Result<()>;
    
    fn get_upnp_friendly_name_prefix(&self) -> Result<String>;
    fn set_upnp_friendly_name_prefix(&self, prefix: String) -> Result<()>;
}
```

**Usage** : Différencier plusieurs instances du serveur (dev, prod, test).

## Bonnes pratiques

### 1. Nommage des méthodes

```rust
// ✅ BON : Préfixer avec le nom du service/composant
fn get_qobuz_username(&self) -> Result<String>
fn get_cache_dir(&self, cache_type: &str, default: &str) -> Result<String>
fn get_paradise_enabled(&self) -> Result<bool>

// ❌ MAUVAIS : Nom trop générique
fn get_username(&self) -> Result<String>
fn get_directory(&self) -> Result<String>
fn is_enabled(&self) -> bool
```

### 2. Gestion des erreurs

```rust
// ✅ BON : Retourner Result pour les valeurs obligatoires
fn get_xxx_username(&self) -> Result<String> {
    match self.get_value(&["accounts", "xxx", "username"])? {
        Value::String(s) => Ok(s),
        _ => Err(anyhow!("XXX username not configured")),
    }
}

// ✅ BON : Retourner Option pour les valeurs optionnelles
fn get_xxx_token(&self) -> Result<Option<String>>

// ✅ BON : Retourner bool pour les checks (sans erreur)
fn is_xxx_valid(&self) -> bool

// ❌ MAUVAIS : Panic ou unwrap
fn get_xxx_value(&self) -> String {
    self.get_value(&["path"]).unwrap().as_str().unwrap()
}
```

### 3. Valeurs par défaut

```rust
// ✅ BON : Constantes en haut du fichier
const DEFAULT_CACHE_SIZE: usize = 500;
const DEFAULT_CACHE_DIR: &str = "cache_audio";

// ✅ BON : Valeurs par défaut documentées
/// Récupère la taille du cache
///
/// # Returns
///
/// Le nombre maximal d'éléments (default: 500)
fn get_cache_size(&self) -> Result<usize>

// ❌ MAUVAIS : Magic numbers
fn get_cache_size(&self) -> Result<usize> {
    match self.get_value(&["cache", "size"]) {
        Ok(Value::Number(n)) => Ok(n.as_u64().unwrap() as usize),
        _ => Ok(500), // Où vient ce 500 ?
    }
}
```

### 4. Documentation

Chaque méthode doit avoir :

```rust
/// Description courte de ce que fait la méthode
///
/// # Arguments (si applicable)
///
/// * `param` - Description du paramètre
///
/// # Returns
///
/// Description de ce qui est retourné (avec valeur par défaut si applicable)
///
/// # Errors (si Result)
///
/// Description des cas d'erreur
///
/// # Exemple
///
/// ```rust,ignore
/// use pmoconfig::get_config;
/// use pmoxxx::XxxConfigExt;
///
/// let config = get_config();
/// let value = config.get_xxx_value()?;
/// ```
fn get_xxx_value(&self) -> Result<Type>;
```

### 5. Organisation du code

#### Structure du fichier config_ext.rs

```rust
//! Documentation du module

// Imports
use anyhow::Result;
use pmoconfig::Config;
use serde_yaml::Value;

// Constantes
const DEFAULT_XXX: Type = value;

// Trait
pub trait XxxConfigExt {
    // Méthodes groupées logiquement
}

// Implémentation
impl XxxConfigExt for Config {
    // Méthodes dans le même ordre que le trait
}

// Tests (optionnel)
#[cfg(test)]
mod tests {
    use super::*;
}
```

#### Ordre des méthodes dans le trait

1. Getters/setters simples
2. Getters/setters combinés
3. Helpers de validation
4. Factory methods
5. Méthodes de nettoyage

```rust
pub trait QobuzConfigExt {
    // 1. Getters/setters simples
    fn get_qobuz_username(&self) -> Result<String>;
    fn set_qobuz_username(&self, username: &str) -> Result<()>;
    fn get_qobuz_password(&self) -> Result<String>;
    fn set_qobuz_password(&self, password: &str) -> Result<()>;
    
    // 2. Getters/setters combinés
    fn get_qobuz_credentials(&self) -> Result<(String, String)>;
    fn set_qobuz_auth_info(&self, ...) -> Result<()>;
    
    // 3. Helpers de validation
    fn is_qobuz_auth_valid(&self) -> bool;
    
    // 4. Factory methods
    fn create_qobuz_client(&self) -> Result<QobuzClient>;
    
    // 5. Méthodes de nettoyage
    fn clear_qobuz_auth_info(&self) -> Result<()>;
}
```

### 6. Types de retour

```rust
// ✅ BON : Result<T> pour les opérations qui peuvent échouer
fn get_xxx_username(&self) -> Result<String>

// ✅ BON : Result<Option<T>> pour les valeurs optionnelles
fn get_xxx_token(&self) -> Result<Option<String>>

// ✅ BON : bool pour les checks simples
fn is_xxx_enabled(&self) -> bool

// ✅ BON : Result<(T1, T2)> pour retourner plusieurs valeurs liées
fn get_xxx_credentials(&self) -> Result<(String, String)>

// ❌ MAUVAIS : Option<Result<T>> (ordre inversé)
fn get_xxx_value(&self) -> Option<Result<String>>
```

### 7. Conversion de types

```rust
// ✅ BON : Gérer plusieurs types d'entrée
fn get_xxx_value(&self) -> Result<u64> {
    match self.get_value(&["path", "to", "value"]) {
        Ok(Value::Number(n)) if n.is_u64() => Ok(n.as_u64().unwrap()),
        Ok(Value::Number(n)) if n.is_i64() => Ok(n.as_i64().unwrap() as u64),
        Ok(Value::String(s)) => s.parse::<u64>()
            .map_err(|e| anyhow!("Invalid number: {}", e)),
        _ => Err(anyhow!("Value not configured")),
    }
}

// ✅ BON : Convertir en format convivial pour l'utilisateur
fn set_xxx_channel(&self, channel: u8) -> Result<()> {
    // Stocker "main" au lieu de "0" dans le YAML
    let name = match channel {
        0 => "main",
        1 => "mellow",
        2 => "rock",
        _ => return Err(anyhow!("Invalid channel")),
    };
    self.set_value(&["path"], Value::String(name.to_string()))
}
```

## Exemples d'implémentation complète

### Exemple 1 : Cache simple (pmocovers)

```rust
//! Extension pour intégrer le cache de couvertures dans pmoconfig

use anyhow::Result;
use pmocache::CacheConfigExt;
use pmoconfig::Config;
use std::sync::Arc;

const DEFAULT_COVER_CACHE_DIR: &str = "cache_covers";
const DEFAULT_COVER_CACHE_SIZE: usize = 2000;

pub trait CoverCacheConfigExt {
    fn get_covers_dir(&self) -> Result<String>;
    fn set_covers_dir(&self, directory: String) -> Result<()>;
    fn get_covers_size(&self) -> Result<usize>;
    fn set_covers_size(&self, size: usize) -> Result<()>;
    fn create_cover_cache(&self) -> Result<Arc<crate::Cache>>;
}

impl CoverCacheConfigExt for Config {
    fn get_covers_dir(&self) -> Result<String> {
        self.get_cache_dir("cover_cache", DEFAULT_COVER_CACHE_DIR)
    }

    fn set_covers_dir(&self, directory: String) -> Result<()> {
        self.set_cache_dir("cover_cache", directory)
    }

    fn get_covers_size(&self) -> Result<usize> {
        self.get_cache_size("cover_cache", DEFAULT_COVER_CACHE_SIZE)
    }

    fn set_covers_size(&self, size: usize) -> Result<()> {
        self.set_cache_size("cover_cache", size)
    }

    fn create_cover_cache(&self) -> Result<Arc<crate::Cache>> {
        let dir = self.get_covers_dir()?;
        let size = self.get_covers_size()?;
        Ok(Arc::new(crate::cache::new_cache(&dir, size)?))
    }
}
```

### Exemple 2 : Service avec authentification (pmoqobuz - simplifié)

```rust
//! Extension pour intégrer la configuration Qobuz dans pmoconfig

use anyhow::{anyhow, Result};
use pmoconfig::Config;
use serde_yaml::Value;

pub trait QobuzConfigExt {
    // Credentials
    fn get_qobuz_username(&self) -> Result<String>;
    fn set_qobuz_username(&self, username: &str) -> Result<()>;
    fn get_qobuz_password(&self) -> Result<String>;
    fn set_qobuz_password(&self, password: &str) -> Result<()>;
    fn get_qobuz_credentials(&self) -> Result<(String, String)>;
    
    // Authentification
    fn get_qobuz_auth_token(&self) -> Result<Option<String>>;
    fn get_qobuz_user_id(&self) -> Result<Option<String>>;
    fn set_qobuz_auth_info(&self, token: &str, user_id: &str) -> Result<()>;
    fn clear_qobuz_auth_info(&self) -> Result<()>;
    fn is_qobuz_auth_valid(&self) -> bool;
}

impl QobuzConfigExt for Config {
    fn get_qobuz_username(&self) -> Result<String> {
        match self.get_value(&["accounts", "qobuz", "username"])? {
            Value::String(s) => Ok(s),
            _ => Err(anyhow!("Qobuz username not configured")),
        }
    }

    fn set_qobuz_username(&self, username: &str) -> Result<()> {
        self.set_value(
            &["accounts", "qobuz", "username"],
            Value::String(username.to_string()),
        )
    }

    fn get_qobuz_password(&self) -> Result<String> {
        match self.get_value(&["accounts", "qobuz", "password"])? {
            Value::String(s) => {
                // Déchiffrement automatique
                pmoconfig::encryption::get_password(&s)
                    .map_err(|e| anyhow!("Failed to decrypt password: {}", e))
            }
            _ => Err(anyhow!("Qobuz password not configured")),
        }
    }

    fn set_qobuz_password(&self, password: &str) -> Result<()> {
        self.set_value(
            &["accounts", "qobuz", "password"],
            Value::String(password.to_string()),
        )
    }

    fn get_qobuz_credentials(&self) -> Result<(String, String)> {
        let username = self.get_qobuz_username()?;
        let password = self.get_qobuz_password()?;
        Ok((username, password))
    }

    fn get_qobuz_auth_token(&self) -> Result<Option<String>> {
        match self.get_value(&["accounts", "qobuz", "auth_token"]) {
            Ok(Value::String(s)) if !s.is_empty() => Ok(Some(s)),
            _ => Ok(None),
        }
    }

    fn get_qobuz_user_id(&self) -> Result<Option<String>> {
        match self.get_value(&["accounts", "qobuz", "user_id"]) {
            Ok(Value::String(s)) if !s.is_empty() => Ok(Some(s)),
            _ => Ok(None),
        }
    }

    fn set_qobuz_auth_info(&self, token: &str, user_id: &str) -> Result<()> {
        self.set_value(
            &["accounts", "qobuz", "auth_token"],
            Value::String(token.to_string()),
        )?;
        self.set_value(
            &["accounts", "qobuz", "user_id"],
            Value::String(user_id.to_string()),
        )?;
        Ok(())
    }

    fn clear_qobuz_auth_info(&self) -> Result<()> {
        let _ = self.set_value(
            &["accounts", "qobuz", "auth_token"],
            Value::String(String::new()),
        );
        let _ = self.set_value(
            &["accounts", "qobuz", "user_id"],
            Value::String(String::new()),
        );
        Ok(())
    }

    fn is_qobuz_auth_valid(&self) -> bool {
        self.get_qobuz_auth_token()
            .ok()
            .flatten()
            .map(|t| !t.is_empty())
            .unwrap_or(false)
            && self
                .get_qobuz_user_id()
                .ok()
                .flatten()
                .map(|u| !u.is_empty())
                .unwrap_or(false)
    }
}
```

### Exemple 3 : Configuration minimale (pmoparadise)

```rust
//! Extension pour intégrer Radio Paradise dans pmoconfig

use anyhow::Result;
use pmoconfig::Config;
use serde_yaml::Value;

pub trait RadioParadiseConfigExt {
    fn get_paradise_enabled(&self) -> Result<bool>;
    fn set_paradise_enabled(&self, enabled: bool) -> Result<()>;
    fn get_paradise_default_channel(&self) -> Result<u8>;
    fn set_paradise_default_channel(&self, channel: u8) -> Result<()>;
}

impl RadioParadiseConfigExt for Config {
    fn get_paradise_enabled(&self) -> Result<bool> {
        match self.get_value(&["sources", "radio_paradise", "enabled"]) {
            Ok(Value::Bool(b)) => Ok(b),
            _ => {
                // Auto-persist le défaut
                self.set_paradise_enabled(true)?;
                Ok(true)
            }
        }
    }

    fn set_paradise_enabled(&self, enabled: bool) -> Result<()> {
        self.set_value(
            &["sources", "radio_paradise", "enabled"],
            Value::Bool(enabled),
        )
    }

    fn get_paradise_default_channel(&self) -> Result<u8> {
        match self.get_value(&["sources", "radio_paradise", "default_channel"]) {
            Ok(Value::String(s)) => {
                // Accepter les noms conviviaux
                match s.as_str() {
                    "main" => Ok(0),
                    "mellow" => Ok(1),
                    "rock" => Ok(2),
                    "eclectic" => Ok(3),
                    _ => {
                        self.set_paradise_default_channel(0)?;
                        Ok(0)
                    }
                }
            }
            Ok(Value::Number(n)) if n.is_u64() => {
                let ch = n.as_u64().unwrap();
                if ch <= 3 {
                    Ok(ch as u8)
                } else {
                    self.set_paradise_default_channel(0)?;
                    Ok(0)
                }
            }
            _ => {
                self.set_value(
                    &["sources", "radio_paradise", "default_channel"],
                    Value::String("main".to_string()),
                )?;
                Ok(0)
            }
        }
    }

    fn set_paradise_default_channel(&self, channel: u8) -> Result<()> {
        let name = match channel {
            0 => "main",
            1 => "mellow",
            2 => "rock",
            3 => "eclectic",
            _ => return Err(anyhow::anyhow!("Invalid channel ID: {}", channel)),
        };
        self.set_value(
            &["sources", "radio_paradise", "default_channel"],
            Value::String(name.to_string()),
        )
    }
}
```

## Intégration dans un crate

### Structure recommandée

```
pmoxxx/
├── Cargo.toml
├── src/
│   ├── lib.rs         # Exporte le trait d'extension
│   ├── config_ext.rs  # Implémentation du trait
│   └── ...           # Reste du code du crate
```

### Dans Cargo.toml

```toml
[dependencies]
pmoconfig = { path = "../pmoconfig" }
anyhow = "1.0"
serde_yaml = "0.9"

# Si c'est un cache, inclure pmocache
pmocache = { path = "../pmocache", optional = false }
```

### Dans lib.rs

```rust
// Exporter le trait pour qu'il soit utilisable
pub mod config_ext;
pub use config_ext::XxxConfigExt;

// Le reste du code du crate
// ...
```

### Utilisation dans le code applicatif

```rust
use pmoconfig::get_config;
use pmoxxx::XxxConfigExt;

fn main() -> anyhow::Result<()> {
    let config = get_config();
    
    // Utiliser les méthodes du trait d'extension
    let value = config.get_xxx_value()?;
    config.set_xxx_value(new_value)?;
    
    // Factory method
    let client = config.create_xxx_client()?;
    
    Ok(())
}
```

## Checklist pour créer un nouveau trait d'extension

- [ ] Créer le fichier `src/config_ext.rs` dans le crate
- [ ] Définir les constantes pour les valeurs par défaut
- [ ] Créer le trait `XxxConfigExt` avec documentation
- [ ] Implémenter les getters avec gestion d'erreur appropriée
- [ ] Implémenter les setters
- [ ] Ajouter les helpers de validation si nécessaire
- [ ] Ajouter les factory methods si applicable
- [ ] Documenter chaque méthode avec exemples
- [ ] Exporter le trait dans `lib.rs`
- [ ] Ajouter `pmoconfig` dans `Cargo.toml`
- [ ] Tester l'intégration

## Philosophie du pattern

### Avantages

1. **Séparation des préoccupations** : Chaque crate gère sa propre configuration
2. **Type safety** : Les erreurs de type sont détectées à la compilation
3. **Extensibilité** : Facile d'ajouter de nouveaux crates sans modifier pmoconfig
4. **Cohérence** : Pattern uniforme dans tout le projet
5. **Documentation** : Interface self-documenting avec exemples

### Principes directeurs

1. **Minimalisme** : Ne configurer que ce qui doit vraiment l'être
2. **Defaults intelligents** : Valeurs par défaut sensées et documentées
3. **Auto-persistence** : Les valeurs importantes sont persistées automatiquement
4. **User-friendly** : Noms conviviaux dans le YAML (strings au lieu de nombres)
5. **Fail-safe** : Gestion des erreurs gracieuse avec fallback sur défauts
6. **Zero surprise** : Comportement prévisible et cohérent

## Sécurité : Chiffrement des mots de passe

Tous les mots de passe dans la configuration doivent pouvoir être chiffrés. Voir `pmoconfig/PASSWORD_ENCRYPTION.md` pour les détails.

### Pattern pour les mots de passe

```rust
fn get_xxx_password(&self) -> Result<String> {
    match self.get_value(&["accounts", "xxx", "password"])? {
        Value::String(s) => {
            // Déchiffrement automatique
            pmoconfig::encryption::get_password(&s)
                .map_err(|e| anyhow!("Failed to decrypt password: {}", e))
        }
        _ => Err(anyhow!("Password not configured")),
    }
}

fn set_xxx_password(&self, password: &str) -> Result<()> {
    // Le chiffrement est fait manuellement par l'utilisateur avec l'outil
    self.set_value(
        &["accounts", "xxx", "password"],
        Value::String(password.to_string()),
    )
}
```

**Important** : Le setter stocke le mot de passe tel quel. C'est l'utilisateur qui décide de le chiffrer ou non avec l'outil `encrypt_password`.

## Références

### Fichiers d'exemple à consulter

- **Cache générique** : `pmocache/src/config_ext.rs`
- **Cache spécialisé** : `pmocovers/src/config_ext.rs` ou `pmoaudiocache/src/config_ext.rs`
- **Service avec auth** : `pmoqobuz/src/config_ext.rs`
- **Configuration minimale** : `pmoparadise/src/config_ext.rs`
- **Configuration UPnP** : `pmoupnp/src/config_ext.rs`
- **Chiffrement** : `pmoconfig/PASSWORD_ENCRYPTION.md`

### Documentation pmoconfig

- `pmoconfig::Config::get_value()`
- `pmoconfig::Config::set_value()`
- `pmoconfig::Config::get_managed_dir()`
- `pmoconfig::encryption` module
