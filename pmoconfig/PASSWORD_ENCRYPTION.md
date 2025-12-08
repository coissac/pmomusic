# Chiffrement des mots de passe dans la configuration

## Vue d'ensemble

`pmoconfig` fournit un système de chiffrement transparent des mots de passe basé sur l'**UUID matériel de la machine**. Cette approche offre un bon compromis entre sécurité et simplicité d'utilisation.

## Principe de fonctionnement

### Clé de chiffrement dérivée de la machine

- La clé de chiffrement AES-256 est dérivée de l'UUID matériel de votre machine
- Sur macOS : utilise `IOPlatformUUID` (via `ioreg`)
- Sur Linux : utilise `/etc/machine-id` ou `/var/lib/dbus/machine-id`
- Sur Windows : utilise l'UUID du BIOS (via `wmic`)

### Algorithme

- **Chiffrement** : AES-256-GCM (Authenticated Encryption)
- **Dérivation de clé** : SHA-256 sur UUID machine + salt
- **Nonce** : Dérivé du mot de passe (chiffrement déterministe)
- **Format** : `encrypted:BASE64(nonce + ciphertext)`

### Avantages

✅ **Pas de keyring** - Aucune dépendance système complexe
✅ **Transparent** - Pas de clé maître à gérer
✅ **Machine-specific** - Le fichier config chiffré ne fonctionne que sur cette machine
✅ **Déchiffrement automatique** - Détection automatique du format
✅ **Déterministe** - Même password = même ciphertext (évite les modifications inutiles du fichier)

### Inconvénients

⚠️ **Non portable** - Le fichier config ne fonctionne pas sur une autre machine
⚠️ **Sécurité limitée** - Un utilisateur avec accès physique peut déchiffrer
⚠️ **Pas de rotation** - Si l'UUID change, les mots de passe deviennent inaccessibles

## Utilisation

### 1. Chiffrer un mot de passe

```bash
cd pmoconfig
cargo run --example encrypt_password -- encrypt "MonMotDePasse123"
```

**Sortie** :
```
Original:  MonMotDePasse123
Encrypted: encrypted:yRyu/jNlJRSdVz0eE+JX56UC2Tk016TmESDoLT6npLBJB3ZuhJ0XTqNOQjiXkkcB

Add this to your config.yaml:
password: "encrypted:yRyu/jNlJRSdVz0eE+JX56UC2Tk016TmESDoLT6npLBJB3ZuhJ0XTqNOQjiXkkcB"
```

### 2. Mettre à jour le fichier config.yaml

Remplacez le mot de passe en clair par la version chiffrée :

**Avant** :
```yaml
accounts:
  qobuz:
    username: user@example.com
    password: MonMotDePasse123
```

**Après** :
```yaml
accounts:
  qobuz:
    username: user@example.com
    password: encrypted:yRyu/jNlJRSdVz0eE+JX56UC2Tk016TmESDoLT6npLBJB3ZuhJ0XTqNOQjiXkkcB
```

### 3. Déchiffrement automatique

Le code de l'application déchiffre automatiquement les mots de passe :

```rust
use pmoconfig::get_config;
use pmoqobuz::QobuzConfigExt;

let config = get_config();

// Déchiffrement automatique si le password commence par "encrypted:"
let password = config.get_qobuz_password()?;
// password contient le mot de passe en clair
```

### 4. Tester le chiffrement

```bash
cargo run --example encrypt_password -- test
```

Cette commande teste le chiffrement/déchiffrement avec différents mots de passe.

### 5. Déchiffrer un mot de passe manuellement

```bash
cargo run --example encrypt_password -- decrypt "encrypted:ABC123..."
```

**Note** : Cela ne fonctionnera que sur la machine où le mot de passe a été chiffré.

## Format du mot de passe chiffré

```
encrypted:BASE64(nonce || ciphertext)
│          │     │       └─ Données chiffrées (longueur variable)
│          │     └─ Nonce de 12 bytes (96 bits)
│          └─ Encodage Base64
└─ Préfixe pour identifier les passwords chiffrés
```

## API de chiffrement

### Fonctions principales

```rust
use pmoconfig::encryption;

// Chiffrer un mot de passe
let encrypted = encryption::encrypt_password("secret")?;
// encrypted = "encrypted:ABC123..."

// Déchiffrer un mot de passe
let password = encryption::decrypt_password(&encrypted)?;
// password = "secret"

// Déchiffrement automatique (gère plaintext et encrypted)
let password = encryption::get_password("encrypted:ABC123...")?;
let password = encryption::get_password("plaintext")?; // Retourne tel quel

// Vérifier si un mot de passe est chiffré
if encryption::is_encrypted(&value) {
    // C'est un mot de passe chiffré
}
```

## Migration progressive

Le système supporte à la fois les mots de passe en clair et chiffrés. Vous pouvez migrer progressivement :

1. **Phase 1** : Le système fonctionne avec des mots de passe en clair
2. **Phase 2** : Chiffrez les mots de passe avec l'outil
3. **Phase 3** : Mettez à jour config.yaml avec les versions chiffrées
4. **Phase 4** : L'application déchiffre automatiquement

Le code fonctionne avec les deux formats, vous n'avez donc pas besoin de tout migrer en même temps.

## Sécurité

### Protection offerte

- ✅ Protection contre la lecture directe du fichier config.yaml
- ✅ Protection si le fichier config est accidentellement partagé/commité
- ✅ Protection contre l'inspection casual du système de fichiers

### Limitations

- ❌ **Pas de protection contre un utilisateur root** - root peut lire l'UUID et déchiffrer
- ❌ **Pas de protection physique** - Quelqu'un avec accès physique peut extraire l'UUID
- ❌ **Pas de protection contre les malwares** - Un malware peut lire l'UUID et déchiffrer

### Recommandations

Pour une sécurité maximale, utilisez plutôt :
- **macOS** : Keychain (`security add-generic-password`)
- **Linux** : Secret Service API (GNOME Keyring, KWallet)
- **Windows** : Credential Manager

Cette implémentation est un **compromis pragmatique** pour :
- Éviter les dépendances lourdes (keyring, etc.)
- Fonctionner sur tous les OS
- Être simple et transparent
- Offrir une protection de base

## Dépannage

### "Decryption failed (wrong machine or corrupted data)"

Ce message apparaît si :
- Le mot de passe a été chiffré sur une autre machine
- L'UUID de la machine a changé (réinstallation OS, nouvelle carte mère)
- Les données sont corrompues

**Solution** : Rechiffrez le mot de passe sur cette machine.

### "Invalid encrypted password format"

Le mot de passe ne commence pas par `encrypted:` ou le format Base64 est invalide.

**Solution** : Vérifiez le format du mot de passe dans config.yaml.

### "Failed to extract IOPlatformUUID" (macOS)

Impossible de lire l'UUID de la machine.

**Solution** : Vérifiez que vous avez les droits d'exécuter `ioreg`.

## Exemple complet

```rust
// Dans pmoqobuz/src/config_ext.rs

impl QobuzConfigExt for Config {
    fn get_qobuz_password(&self) -> Result<String> {
        match self.get_value(&["accounts", "qobuz", "password"])? {
            Value::String(s) => {
                // Déchiffrement automatique si le mot de passe est chiffré
                pmoconfig::encryption::get_password(&s)
                    .map_err(|e| anyhow!("Failed to decrypt password: {}", e))
            }
            _ => Err(anyhow!("Qobuz password not configured")),
        }
    }
}
```

## Tests

```bash
# Tester le module de chiffrement
cargo test -p pmoconfig encryption

# Tester l'outil CLI
cargo run --example encrypt_password -- test

# Tester avec un vrai service (Qobuz)
cargo run --example basic_usage
```
