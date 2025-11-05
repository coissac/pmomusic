# Installation des dépendances système sans droits sudo

Ce document explique comment installer les dépendances système de `pmoaudio` localement sans privilèges administrateur.

## Dépendances requises

1. **libsoxr** - Nécessaire pour `ResamplingNode` (resampling audio haute qualité)
2. **libasound2** (ALSA) - Nécessaire pour `AudioSink` via rodio (lecture audio sur Linux)

## Contexte

Les crates `soxr` et `rodio` nécessitent des bibliothèques système. Dans un environnement sans droits sudo, voici comment les installer localement.

## Méthode : Installation locale via apt-get download

### 1. Télécharger les packages .deb

```bash
cd ~/.local

# Pour libsoxr (ResamplingNode)
apt-get download libsoxr-dev libsoxr0

# Pour ALSA (AudioSink)
apt-get download libasound2-dev
```

Cela télécharge les fichiers `.deb` sans les installer système-wide.

### 2. Extraire les packages

```bash
# Extraire libsoxr
dpkg -x libsoxr-dev_*.deb .
dpkg -x libsoxr0_*.deb .

# Extraire ALSA
dpkg -x libasound2-dev_*.deb .
```

Les fichiers sont extraits dans `~/.local/usr/lib/x86_64-linux-gnu/` et `~/.local/usr/include/`.

### 3. Configurer les variables d'environnement

Ajouter à votre `~/.bashrc` ou exporter dans votre session :

```bash
export PKG_CONFIG_PATH="/root/.local/usr/lib/x86_64-linux-gnu/pkgconfig:$PKG_CONFIG_PATH"
export LD_LIBRARY_PATH="/root/.local/usr/lib/x86_64-linux-gnu:$LD_LIBRARY_PATH"
```

**IMPORTANT:** Remplacer `/root/` par le chemin de votre home directory (`$HOME` ou `~`).

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

## Notes pour Claude Code sessions

Pour les futures sessions Claude :

1. Exporter les variables d'environnement en début de session
2. NE PAS créer de fichiers `.cargo/config.toml` dans le projet
3. Si nécessaire pour les tests, les créer localement mais ne pas les committer
4. Documenter toute difficulté d'installation ici

## Références

- libsoxr GitHub: https://github.com/chirlu/soxr
- Documentation pkg-config: https://www.freedesktop.org/wiki/Software/pkg-config/
