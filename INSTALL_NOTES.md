# Notes d'installation pour PMOMusic

## Prérequis système

### libsoxr (obligatoire pour pmoaudio)

La bibliothèque `libsoxr` est requise pour le resampling audio dans `pmoaudio`.

**Installation** :

```bash
# Debian/Ubuntu
sudo apt-get install libsoxr-dev

# Fedora/RHEL
sudo dnf install libsoxr-devel

# Arch Linux
sudo pacman -S libsoxr

# macOS (Homebrew)
brew install libsoxr

# Alpine Linux
apk add soxr-dev
```

**Sans privilèges root** : Si vous n'avez pas les droits sudo, demandez à l'administrateur système d'installer `libsoxr-dev`.

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
