# Installation de libsoxr sans droits sudo

Ce document explique comment installer libsoxr localement sans privilèges administrateur, nécessaire pour compiler `pmoaudio` avec le support de resampling.

## Contexte

Le crate `soxr` (utilisé par `ResamplingNode`) nécessite la bibliothèque système `libsoxr`. Dans un environnement sans droits sudo, voici comment l'installer localement.

## Méthode : Installation locale via apt-get download

### 1. Télécharger les packages .deb

```bash
cd ~/.local
apt-get download libsoxr-dev libsoxr0
```

Cela télécharge les fichiers `.deb` sans les installer système-wide.

### 2. Extraire les packages

```bash
dpkg -x libsoxr-dev_*.deb .
dpkg -x libsoxr0_*.deb .
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
pkg-config --libs --cflags soxr
```

Devrait retourner :
```
-I/root/.local/usr/include -L/root/.local/usr/lib/x86_64-linux-gnu -lsoxr
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
```

### Debian/Ubuntu (avec sudo)

```bash
sudo apt-get install libsoxr-dev
```

### Fedora/RHEL

```bash
sudo dnf install soxr-devel
```

## Troubleshooting

### Erreur : "Package 'soxr' was not found"

- Vérifier que `PKG_CONFIG_PATH` contient le bon chemin
- Vérifier que le fichier `soxr.pc` existe dans ce répertoire

### Erreur de link : "unable to find library -lsoxr"

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
