# Notes d'installation pour PMOMusic

## Prérequis système

### libsoxr (obligatoire pour pmoaudio - resampling)

La bibliothèque `libsoxr` est requise pour le resampling audio dans `pmoaudio`.

### libasound2/ALSA (obligatoire pour pmoaudio - lecture audio sur Linux)

La bibliothèque ALSA est requise pour `AudioSink` via `cpal` sur Linux. Sur macOS et Windows, aucune dépendance externe n'est nécessaire (CoreAudio et WASAPI sont utilisés).

**Installation** :

```bash
# Debian/Ubuntu
sudo apt-get install libsoxr-dev libasound2-dev

# Fedora/RHEL
sudo dnf install libsoxr-devel alsa-lib-devel

# Arch Linux
sudo pacman -S libsoxr alsa-lib

# macOS (Homebrew) - ALSA non nécessaire sur macOS
brew install libsoxr

# Alpine Linux
apk add soxr-dev alsa-lib-dev
```

**Sans privilèges root (Claude Code, environnements sans sudo)** :

🚀 **Installation automatique** :

```bash
# 1. Installation des dépendances (une seule fois)
./setup-deps.sh

# 2. Configuration de l'environnement (à chaque session)
source setup-env.sh

# 3. Compilation
cargo build
```

Pour plus de détails, consultez `INSTALL_LIBSOXR.md`.

---

## Nouveaux composants

### PlaylistSource (pmoaudio-ext)

Source audio qui lit une playlist `pmoplaylist` et diffuse les pistes en continu.

**Feature** : `playlist`

```bash
# Compiler avec la feature playlist
cargo build --package pmoaudio-ext --features playlist
```

**⚠️ Important** : Cette source émet du PCM avec sample_rate et bit_depth **variables**. Pour un flux homogène, ajoutez dans le pipeline :
- `ResamplingNode` (normalise le sample_rate)
- `ToI24Node` / `ToI16Node` (normalise la profondeur de bits)

### ResamplingNode (pmoaudio)

Nœud générique qui normalise le sample_rate vers une valeur cible fixe.

**Usage** :
```rust
let mut resampler = ResamplingNode::new(48000); // Force 48kHz
```

---

## Compilation

```bash
# Compiler tout le workspace (nécessite libsoxr)
cargo build

# Compiler sans pmoaudio (si libsoxr manque)
cargo build --package pmoplaylist
cargo build --package pmoaudiocache
# etc.
```
