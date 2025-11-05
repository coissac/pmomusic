# Installation des dépendances système sans droits sudo

Ce document explique comment installer les dépendances système de `pmoaudio` localement sans privilèges administrateur.

## Dépendances requises

1. **libsoxr** - Nécessaire pour `ResamplingNode` (resampling audio haute qualité)
2. **libasound2** (ALSA) - Nécessaire pour `AudioSink` via cpal (lecture audio sur Linux)

## Contexte

Les crates `soxr` et `cpal` nécessitent des bibliothèques système. Dans un environnement sans droits sudo (comme Claude Code), voici comment les installer localement.

## Méthode : Installation locale via apt-get download

### 1. Télécharger les packages .deb

```bash
cd ~/.local

# Pour libsoxr (ResamplingNode)
apt-get download libsoxr-dev libsoxr0

# Pour ALSA (AudioSink)
# Note: libasound2t64 contient la bibliothèque partagée, libasound2-dev les headers
apt-get download libasound2-dev libasound2t64
```

Cela télécharge les fichiers `.deb` sans les installer système-wide.

### 2. Extraire les packages

```bash
# Extraire libsoxr
dpkg -x libsoxr-dev_*.deb .
dpkg -x libsoxr0_*.deb .

# Extraire ALSA
dpkg -x libasound2-dev_*.deb .
dpkg -x libasound2t64_*.deb .
```

Les fichiers sont extraits dans `~/.local/usr/lib/x86_64-linux-gnu/` et `~/.local/usr/include/`.

### 3. Configurer les variables d'environnement

Ajouter à votre `~/.bashrc` ou exporter dans votre session :

```bash
export PKG_CONFIG_PATH="$HOME/.local/usr/lib/x86_64-linux-gnu/pkgconfig:$PKG_CONFIG_PATH"
export LD_LIBRARY_PATH="$HOME/.local/usr/lib/x86_64-linux-gnu:$LD_LIBRARY_PATH"
export RUSTFLAGS="-L $HOME/.local/usr/lib/x86_64-linux-gnu"
```

**IMPORTANT:** Ces variables doivent être définies dans chaque session où vous compilez le projet.

### 4. Vérifier l'installation

```bash
# Vérifier libsoxr
pkg-config --libs --cflags soxr

# Vérifier ALSA
pkg-config --libs --cflags alsa
```

Devrait retourner quelque chose comme :
```
# soxr
-I/root/.local/usr/include -L/root/.local/usr/lib/x86_64-linux-gnu -lsoxr

# alsa
-I/root/.local/usr/include -L/root/.local/usr/lib/x86_64-linux-gnu -lasound
```

## Utilisation avec Cargo

### Pour les builds réguliers

Les variables d'environnement suffisent pour `cargo build` et `cargo run`.

### Pour les tests

Les tests nécessitent également la configuration du linker. Deux options :

#### Option A : Configuration locale du projet (NON RECOMMANDÉ pour le versioning)

Créer `.cargo/config.toml` dans chaque crate :

```toml
[build]
rustflags = ["-L", "/root/.local/usr/lib/x86_64-linux-gnu"]
```

**⚠️ NE PAS committer ces fichiers** - ils contiennent des chemins spécifiques à votre installation.

#### Option B : Variables d'environnement pour cargo test

```bash
export PKG_CONFIG_PATH="$HOME/.local/usr/lib/x86_64-linux-gnu/pkgconfig:$PKG_CONFIG_PATH"
export LD_LIBRARY_PATH="$HOME/.local/usr/lib/x86_64-linux-gnu:$LD_LIBRARY_PATH"
cargo test
```

## Pour d'autres distributions

### macOS (avec Homebrew)

```bash
brew install libsoxr
# Note: ALSA n'est pas nécessaire sur macOS (rodio utilise CoreAudio)
```

### Debian/Ubuntu (avec sudo)

```bash
sudo apt-get install libsoxr-dev libasound2-dev
```

### Fedora/RHEL

```bash
sudo dnf install soxr-devel
```

## Troubleshooting

### Erreur : "Package 'soxr' was not found" ou "Package 'alsa' was not found"

- Vérifier que `PKG_CONFIG_PATH` contient le bon chemin
- Vérifier que les fichiers `soxr.pc` et `alsa.pc` existent dans ce répertoire

### Erreur de link : "unable to find library -lsoxr" ou "-lasound"

- Pour `cargo build` : vérifier `LD_LIBRARY_PATH`
- Pour `cargo test` : utiliser la configuration rustflags (Option A ci-dessus)

### Le test compile mais échoue au runtime

```
error while loading shared libraries: libsoxr.so.0: cannot open shared object file
```

Solution : Ajouter `LD_LIBRARY_PATH` également pour l'exécution :

```bash
export LD_LIBRARY_PATH="$HOME/.local/usr/lib/x86_64-linux-gnu:$LD_LIBRARY_PATH"
cargo test
```

## Guide complet pour environnement Claude Code

### Configuration initiale (à faire une seule fois)

Dans une session Claude Code (https://claude.ai/code), vous n'avez pas de droits sudo. Suivez ces étapes :

#### 1. Installation des dépendances

```bash
# Créer le répertoire local
mkdir -p ~/.local
cd ~/.local

# Télécharger tous les packages nécessaires
apt-get download libsoxr-dev libsoxr0 libasound2-dev libasound2t64

# Extraire tous les packages
dpkg -x libsoxr-dev_*.deb .
dpkg -x libsoxr0_*.deb .
dpkg -x libasound2-dev_*.deb .
dpkg -x libasound2t64_*.deb .

# Retourner au projet
cd /home/user/pmomusic
```

#### 2. Configuration des variables d'environnement

**IMPORTANT:** Ces variables doivent être exportées dans CHAQUE session Claude Code avant de compiler :

```bash
export PKG_CONFIG_PATH="$HOME/.local/usr/lib/x86_64-linux-gnu/pkgconfig:$PKG_CONFIG_PATH"
export LD_LIBRARY_PATH="$HOME/.local/usr/lib/x86_64-linux-gnu:$LD_LIBRARY_PATH"
export RUSTFLAGS="-L $HOME/.local/usr/lib/x86_64-linux-gnu"
```

**Astuce :** Copier ces trois lignes dans un fichier `setup-env.sh` à la racine du projet :

```bash
cat > setup-env.sh << 'EOF'
export PKG_CONFIG_PATH="$HOME/.local/usr/lib/x86_64-linux-gnu/pkgconfig:$PKG_CONFIG_PATH"
export LD_LIBRARY_PATH="$HOME/.local/usr/lib/x86_64-linux-gnu:$LD_LIBRARY_PATH"
export RUSTFLAGS="-L $HOME/.local/usr/lib/x86_64-linux-gnu"
EOF
```

Puis dans chaque session :

```bash
source setup-env.sh
```

⚠️ **NE PAS committer `setup-env.sh`** - ajouter au `.gitignore`

#### 3. Vérifier l'installation

```bash
# Vérifier que pkg-config trouve les bibliothèques
pkg-config --libs --cflags soxr
pkg-config --libs --cflags alsa

# Devrait afficher quelque chose comme :
# -I/root/.local/usr/include -L/root/.local/usr/lib/x86_64-linux-gnu -lsoxr
# -I/root/.local/usr/include -L/root/.local/usr/lib/x86_64-linux-gnu -lasound
```

#### 4. Compiler et tester

```bash
# Compiler le workspace complet
cargo build

# Tester l'exemple play_and_cache de pmoparadise
cargo run --package pmoparadise --example play_and_cache --features full -- 0
```

### Workflow pour chaque nouvelle session

À chaque fois que vous démarrez une nouvelle session Claude Code :

1. **Exporter les variables d'environnement** (ou `source setup-env.sh`)
2. Compiler avec `cargo build`
3. Exécuter les exemples ou tests

**IMPORTANT :** Si vous oubliez d'exporter les variables, vous obtiendrez des erreurs comme :
```
error: failed to run custom build command for `soxr-sys`
Package 'soxr' was not found in the pkg-config search path
```

ou

```
rust-lld: error: unable to find library -lasound
```

Solution : Exporter les variables et recompiler.

### Notes importantes

- ✅ Les dépendances installées dans `~/.local` persistent entre les sessions
- ✅ Les variables d'environnement doivent être réexportées à chaque nouvelle session
- ❌ NE JAMAIS créer de fichiers `.cargo/config.toml` dans le projet (chemins spécifiques)
- ❌ NE JAMAIS committer `setup-env.sh` (configuration locale)
- 💡 Sur macOS (via Homebrew) : seul `libsoxr` est nécessaire (pas d'ALSA)

## Références

- libsoxr GitHub: https://github.com/chirlu/soxr
- Documentation pkg-config: https://www.freedesktop.org/wiki/Software/pkg-config/
