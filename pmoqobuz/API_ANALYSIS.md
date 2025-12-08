# Analyse des différences entre l'API Rust et Python

## Vue d'ensemble

L'implémentation actuelle de `pmoqobuz` ne suit pas complètement l'API de référence Python (`qobuz.api.raw`). Voici les principales différences et ce qui doit être corrigé.

## Problèmes identifiés

### 1. ❌ Gestion du secret `s4` manquante

**Python** :
- Accepte soit `appid` + `configvalue` (secret encodé en base64)
- Soit utilise le `Spoofer` pour obtenir l'appID et les secrets dynamiquement
- Le `configvalue` est décodé et XORé avec l'appID pour obtenir le secret `s4`
- Le secret `s4` est utilisé pour signer certaines requêtes critiques

**Rust actuel** :
- ❌ Utilise un `DEFAULT_APP_ID` codé en dur
- ❌ Pas de gestion du secret `s4`
- ❌ Pas d'utilisation du Spoofer pour obtenir l'appID/secret
- ❌ Pas de méthode pour décoder et dériver le secret depuis un `configvalue`

**Impact** :
- Les requêtes `track/getFileUrl` et `userLibrary/getAlbumsList` échoueront probablement car elles nécessitent une signature MD5

### 2. ❌ Signature MD5 des requêtes manquante

**Python - track_getFileUrl** :
```python
ts = str(time.time())
stringvalue = ("trackgetFileUrlformat_id" + fmt_id +
               "intent" + intent +
               "track_id" + track_id + ts).encode("ASCII")
stringvalue += self.s4  # Secret ajouté
rq_sig = str(hashlib.md5(stringvalue).hexdigest())
params = {
    "format_id": fmt_id,
    "intent": intent,
    "request_ts": ts,        # ← Timestamp
    "request_sig": rq_sig,   # ← Signature MD5
    "track_id": track_id,
}
```

**Rust actuel (catalog.rs:210-218)** :
```rust
let params = [
    ("track_id", track_id),
    ("format_id", &format_id),
    ("intent", "stream"),
    // ❌ MANQUE: request_ts
    // ❌ MANQUE: request_sig
];
```

**Impact** :
- Les requêtes de streaming peuvent échouer ou retourner des URLs invalides

### 3. ❌ Méthode `userlib_getAlbums` manquante

**Python** :
```python
def userlib_getAlbums(self, **ka):
    ts = str(time.time())
    r_sig = "userLibrarygetAlbumsList" + str(ts) + str(ka["sec"])
    r_sig_hashed = hashlib.md5(r_sig.encode("utf-8")).hexdigest()
    params = {
        "app_id": self.appid,
        "user_auth_token": self.user_auth_token,
        "request_ts": ts,
        "request_sig": r_sig_hashed,
    }
    return self._api_request(params, "/userLibrary/getAlbumsList")
```

**Rust actuel** :
- ❌ Méthode totalement absente

**Impact** :
- Impossible de tester les secrets (méthode `setSec()`)
- Impossible de récupérer la bibliothèque d'albums de l'utilisateur

### 4. ❌ Méthode `setSec()` manquante

**Python** :
```python
def setSec(self):
    # Teste tous les secrets du spoofer
    for value in self.spoofer.getSecrets().values():
        self.s4 = value.encode("utf-8")
        if self.userlib_getAlbums(sec=self.s4) is not None:
            # Ce secret fonctionne !
            return
```

**Rust actuel** :
- ❌ Méthode totalement absente
- ❌ Pas de mécanisme pour tester et sélectionner le bon secret

**Impact** :
- Si on utilise le Spoofer, impossible de trouver le bon secret parmi ceux retournés

### 5. ⚠️ Configuration incomplète

**Python** :
- Peut être initialisé avec `appid` + `configvalue` OU utiliser le Spoofer

**Rust actuel** :
- ✅ Configuration du username/password via `QobuzConfigExt`
- ❌ Pas de configuration pour `appid` et `secret`/`configvalue`

**Impact** :
- Impossible de configurer manuellement un appID et secret valides
- Dépendance à un appID codé en dur qui peut devenir obsolète

## Plan de correction

### Phase 1: Extension de la configuration

**Fichier: `pmoqobuz/src/config_ext.rs`**

Ajouter au trait `QobuzConfigExt` :
- `get_qobuz_appid()` / `set_qobuz_appid()`
- `get_qobuz_secret()` / `set_qobuz_secret()` (stocke la valeur base64)

### Phase 2: Ajout du support du secret dans QobuzApi

**Fichier: `pmoqobuz/src/api/mod.rs`**

Modifications de `QobuzApi` :
```rust
pub struct QobuzApi {
    client: Client,
    app_id: String,
    secret: Option<Vec<u8>>,  // ← Nouveau : secret s4 décodé
    user_auth_token: Option<String>,
    user_id: Option<String>,
    format_id: AudioFormat,
}
```

Nouvelles méthodes :
```rust
impl QobuzApi {
    /// Crée une API avec appid + configvalue
    pub fn with_secret(app_id: impl Into<String>, configvalue: &str) -> Result<Self>;

    /// Crée une API en utilisant le Spoofer
    pub async fn with_spoofer() -> Result<Self>;

    /// Définit le secret s4
    pub fn set_secret(&mut self, secret: Vec<u8>);

    /// Teste un secret en appelant userlib_getAlbums
    async fn test_secret(&self, secret: &[u8]) -> bool;

    /// Teste et sélectionne le bon secret depuis le Spoofer
    async fn set_secret_from_spoofer(&mut self, spoofer: &Spoofer) -> Result<()>;
}
```

### Phase 3: Implémentation des méthodes signées

**Fichier: `pmoqobuz/src/api/signing.rs` (nouveau)**

```rust
use md5::{Md5, Digest};
use std::time::{SystemTime, UNIX_EPOCH};

/// Génère un timestamp Unix
pub fn get_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
        .to_string()
}

/// Signe une requête track/getFileUrl
pub fn sign_track_get_file_url(
    format_id: &str,
    intent: &str,
    track_id: &str,
    timestamp: &str,
    secret: &[u8],
) -> String {
    let mut hasher = Md5::new();
    hasher.update(b"trackgetFileUrlformat_id");
    hasher.update(format_id.as_bytes());
    hasher.update(b"intent");
    hasher.update(intent.as_bytes());
    hasher.update(b"track_id");
    hasher.update(track_id.as_bytes());
    hasher.update(timestamp.as_bytes());
    hasher.update(secret);
    format!("{:x}", hasher.finalize())
}

/// Signe une requête userLibrary/getAlbumsList
pub fn sign_userlib_get_albums(timestamp: &str, secret: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(b"userLibrarygetAlbumsList");
    hasher.update(timestamp.as_bytes());
    hasher.update(secret);
    format!("{:x}", hasher.finalize())
}
```

**Fichier: `pmoqobuz/src/api/catalog.rs`**

Modifier `get_file_url` :
```rust
pub async fn get_file_url(&self, track_id: &str) -> Result<StreamInfo> {
    let format_id = self.format_id.id().to_string();
    let timestamp = signing::get_timestamp();

    // Signature MD5 requise !
    let secret = self.secret.as_ref()
        .ok_or_else(|| QobuzError::Configuration("Secret not configured".into()))?;

    let signature = signing::sign_track_get_file_url(
        &format_id,
        "stream",
        track_id,
        &timestamp,
        secret,
    );

    let params = [
        ("track_id", track_id),
        ("format_id", format_id.as_str()),
        ("intent", "stream"),
        ("request_ts", timestamp.as_str()),
        ("request_sig", signature.as_str()),
    ];

    let response: FileUrlResponse = self.get("/track/getFileUrl", &params).await?;
    // ...
}
```

**Fichier: `pmoqobuz/src/api/user.rs`**

Ajouter :
```rust
pub async fn get_user_albums(&self) -> Result<UserAlbumsResponse> {
    let timestamp = signing::get_timestamp();

    let secret = self.secret.as_ref()
        .ok_or_else(|| QobuzError::Configuration("Secret not configured".into()))?;

    let signature = signing::sign_userlib_get_albums(&timestamp, secret);

    let params = [
        ("app_id", self.app_id.as_str()),
        ("user_auth_token", self.user_auth_token.as_ref()
            .ok_or_else(|| QobuzError::Unauthorized("Not logged in".into()))?
            .as_str()),
        ("request_ts", timestamp.as_str()),
        ("request_sig", signature.as_str()),
    ];

    self.post("/userLibrary/getAlbumsList", &params).await
}
```

### Phase 4: Modification de QobuzClient

**Fichier: `pmoqobuz/src/client.rs`**

```rust
impl QobuzClient {
    /// Crée un client avec appID et secret depuis la config
    pub async fn from_config() -> Result<Self> {
        let config = pmoconfig::get_config();

        // Essayer d'obtenir appid et secret depuis la config
        let api = if let (Ok(appid), Ok(secret)) = (
            config.get_qobuz_appid(),
            config.get_qobuz_secret()
        ) {
            QobuzApi::with_secret(appid, &secret)?
        } else {
            // Sinon, utiliser le Spoofer
            warn!("AppID/secret not configured, using Spoofer");
            QobuzApi::with_spoofer().await?
        };

        // Login...
        let (username, password) = config.get_qobuz_credentials()?;
        // ...
    }
}
```

## Dépendances à ajouter

**Cargo.toml** :
```toml
md5 = "0.7"
```

## Résumé des fichiers à modifier/créer

### Modifications
- [x] `pmoqobuz/src/config_ext.rs` - Ajouter appid et secret
- [ ] `pmoqobuz/src/api/mod.rs` - Ajouter champ secret et nouvelles méthodes
- [ ] `pmoqobuz/src/api/auth.rs` - Appeler `set_secret_from_spoofer` après login
- [ ] `pmoqobuz/src/api/catalog.rs` - Ajouter signature à `get_file_url`
- [ ] `pmoqobuz/src/api/user.rs` - Ajouter `get_user_albums` avec signature
- [ ] `pmoqobuz/src/client.rs` - Utiliser Spoofer si pas de config
- [ ] `pmoqobuz/Cargo.toml` - Ajouter dépendance `md5`

### Nouveaux fichiers
- [ ] `pmoqobuz/src/api/signing.rs` - Fonctions de signature MD5

## Tests nécessaires

1. **Test avec Spoofer** : Vérifier que l'obtention automatique de l'appID fonctionne
2. **Test avec config manuelle** : Vérifier qu'on peut configurer un appID/secret
3. **Test de signature** : Vérifier que les signatures MD5 sont correctes
4. **Test de setSec** : Vérifier que le bon secret est sélectionné
5. **Test de streaming** : Vérifier qu'on obtient des URLs valides avec `get_file_url`
