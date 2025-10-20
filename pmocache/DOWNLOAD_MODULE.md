# Module Download

Module de téléchargement asynchrone avec support de transformation de stream.

## Vue d'ensemble

Le module `download` permet de télécharger des fichiers depuis une URL en tâche de fond avec :
- Suivi de la progression en temps réel
- Support de transformations de stream (conversion, compression, etc.)
- API non-bloquante avec attentes conditionnelles
- Gestion d'erreurs robuste

## API

### Types principaux

#### `Download`
Objet représentant un téléchargement en cours, partagé via `Arc<Download>`.

**Méthodes:**
- `filename() -> &Path` - Retourne le chemin du fichier de destination
- `wait_until_min_size(size: u64) -> Result<(), String>` - Attend que le fichier atteigne une taille minimale
- `wait_until_finished() -> Result<(), String>` - Attend la fin complète du téléchargement
- `open() -> io::Result<File>` - Ouvre le fichier pour lecture
- `pos() -> u64` - Position de lecture actuelle
- `set_pos(pos: u64)` - Définit la position de lecture
- `expected_size() -> Option<u64>` - Taille attendue du fichier source (via Content-Length)
- `current_size() -> u64` - Taille actuellement téléchargée (source)
- `transformed_size() -> u64` - Taille des données transformées écrites
- `finished() -> bool` - Indique si le téléchargement est terminé
- `error() -> Option<String>` - Retourne l'erreur éventuelle

#### `StreamTransformer`
Type pour une fonction de transformation de stream.

```rust
pub type StreamTransformer = Box<
    dyn FnOnce(
            reqwest::Response,
            tokio::fs::File,
            Arc<dyn Fn(u64) + Send + Sync>,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>
        + Send,
>;
```

**Paramètres:**
1. `reqwest::Response` - La réponse HTTP avec le stream de données
2. `tokio::fs::File` - Le fichier de destination ouvert en écriture
3. `Arc<dyn Fn(u64) + Send + Sync>` - Callback pour mettre à jour la progression (taille transformée)

**Retour:**
- `Future<Output = Result<(), String>>` - Future qui se résout quand la transformation est terminée

### Fonctions

#### `download(filename, url) -> Arc<Download>`
Télécharge un fichier sans transformation.

```rust
use pmocache::download::download;

let dl = download("/tmp/file.dat", "https://example.com/file.dat");
dl.wait_until_finished().await?;
```

#### `download_with_transformer(filename, url, transformer) -> Arc<Download>`
Télécharge un fichier avec une transformation optionnelle du stream.

```rust
use pmocache::download::{download_with_transformer, StreamTransformer};

let transformer: StreamTransformer = Box::new(|response, mut file, update_progress| {
    Box::pin(async move {
        // Votre logique de transformation ici
        Ok(())
    })
});

let dl = download_with_transformer("/tmp/output.dat", "https://example.com/input.dat", Some(transformer));
```

## Exemples d'utilisation

### 1. Téléchargement simple

```rust
use pmocache::download::download;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dl = download("/tmp/rust.html", "https://www.rust-lang.org/");

    println!("Téléchargement démarré...");

    // Attendre au moins 1KB
    dl.wait_until_min_size(1024).await?;
    println!("Au moins 1KB téléchargés");

    // Attendre la fin
    dl.wait_until_finished().await?;
    println!("Terminé! Taille: {} bytes", dl.current_size().await);

    Ok(())
}
```

### 2. Transformation en majuscules

```rust
use pmocache::download::{download_with_transformer, StreamTransformer};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

fn uppercase_transformer() -> StreamTransformer {
    Box::new(|response, mut file, update_progress| {
        Box::pin(async move {
            let mut stream = response.bytes_stream();
            let mut total = 0u64;

            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.map_err(|e| e.to_string())?;

                // Transformer en majuscules
                let uppercase: Vec<u8> = chunk
                    .iter()
                    .map(|&b| b.to_ascii_uppercase())
                    .collect();

                file.write_all(&uppercase).await.map_err(|e| e.to_string())?;

                total += uppercase.len() as u64;
                update_progress(total);
            }

            file.flush().await.map_err(|e| e.to_string())?;
            Ok(())
        })
    })
}

#[tokio::main]
async fn main() {
    let transformer = uppercase_transformer();
    let dl = download_with_transformer("/tmp/UPPERCASE.txt", "https://example.com/text.txt", Some(transformer));

    dl.wait_until_finished().await.unwrap();
    println!("Fichier converti en majuscules!");
}
```

### 3. Compression GZIP à la volée

```rust
use pmocache::download::{download_with_transformer, StreamTransformer};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use async_compression::tokio::write::GzipEncoder;

fn gzip_transformer() -> StreamTransformer {
    Box::new(|response, file, update_progress| {
        Box::pin(async move {
            let mut encoder = GzipEncoder::new(file);
            let mut stream = response.bytes_stream();
            let mut total = 0u64;

            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.map_err(|e| e.to_string())?;
                encoder.write_all(&chunk).await.map_err(|e| e.to_string())?;

                total += chunk.len() as u64;
                update_progress(total);
            }

            encoder.shutdown().await.map_err(|e| e.to_string())?;
            Ok(())
        })
    })
}
```

### 4. Conversion d'image (concept)

```rust
// Exemple conceptuel de conversion WebP
// (nécessiterait une bibliothèque de traitement d'images)

fn webp_transformer() -> StreamTransformer {
    Box::new(|response, mut file, update_progress| {
        Box::pin(async move {
            // 1. Télécharger l'image en mémoire
            let bytes = response.bytes().await.map_err(|e| e.to_string())?;

            // 2. Décoder l'image source
            let img = image::load_from_memory(&bytes)
                .map_err(|e| format!("Failed to decode image: {}", e))?;

            // 3. Encoder en WebP
            let mut webp_data = Vec::new();
            let encoder = webp::Encoder::from_image(&img)
                .map_err(|e| format!("Failed to create WebP encoder: {}", e))?;
            let webp = encoder.encode(75.0); // Qualité 75%
            webp_data.extend_from_slice(&*webp);

            // 4. Écrire le résultat
            file.write_all(&webp_data).await.map_err(|e| e.to_string())?;
            file.flush().await.map_err(|e| e.to_string())?;

            update_progress(webp_data.len() as u64);
            Ok(())
        })
    })
}

// Utilisation
let transformer = webp_transformer();
let dl = download_with_transformer(
    "/tmp/image.webp",
    "https://example.com/image.jpg",
    Some(transformer)
);
```

### 5. Conversion audio (concept)

```rust
// Exemple conceptuel de conversion MP3 -> FLAC
// (nécessiterait des bibliothèques audio comme symphonia)

fn mp3_to_flac_transformer() -> StreamTransformer {
    Box::new(|response, mut file, update_progress| {
        Box::pin(async move {
            // 1. Télécharger le MP3 en mémoire
            let mp3_bytes = response.bytes().await.map_err(|e| e.to_string())?;

            // 2. Décoder le MP3
            let cursor = std::io::Cursor::new(mp3_bytes);
            let mp3_decoder = minimp3::Decoder::new(cursor);

            let mut samples = Vec::new();
            let mut sample_rate = 0;
            let mut channels = 0;

            for frame in mp3_decoder {
                let frame = frame.map_err(|e| format!("MP3 decode error: {:?}", e))?;
                if sample_rate == 0 {
                    sample_rate = frame.sample_rate;
                    channels = frame.channels;
                }
                samples.extend_from_slice(&frame.data);
            }

            // 3. Encoder en FLAC
            let mut flac_encoder = claxon::FlacEncoder::new(
                &mut file,
                sample_rate,
                channels as u32,
                16, // bits per sample
            ).map_err(|e| format!("FLAC encoder error: {:?}", e))?;

            for sample in samples {
                flac_encoder.write_sample(sample as i32)
                    .map_err(|e| format!("FLAC write error: {:?}", e))?;
            }

            flac_encoder.finish()
                .map_err(|e| format!("FLAC finalize error: {:?}", e))?;

            file.flush().await.map_err(|e| e.to_string())?;

            // Note: on ne peut pas facilement connaître la taille finale avant d'avoir tout encodé
            // Pour un suivi précis, il faudrait encoder par chunks
            Ok(())
        })
    })
}
```

## Cas d'usage dans PMOMusic

### 1. Cache audio avec conversion
```rust
// Télécharger du MP3 et le convertir en FLAC pour le cache
let transformer = mp3_to_flac_transformer();
let dl = download_with_transformer(
    cache_path,
    audio_url,
    Some(transformer)
);
```

### 2. Cache d'images avec WebP
```rust
// Télécharger une image et la convertir en WebP
let transformer = webp_transformer();
let dl = download_with_transformer(
    cover_cache_path,
    cover_url,
    Some(transformer)
);
```

### 3. Streaming progressif
```rust
// Commencer à lire le fichier dès qu'on a assez de données
let dl = download(audio_path, stream_url);

// Attendre au moins 256KB pour commencer la lecture
dl.wait_until_min_size(256 * 1024).await?;

// Ouvrir le fichier et commencer à lire pendant que le téléchargement continue
let file = dl.open()?;
// ... lecture du fichier
```

## Notes d'implémentation

### Thread safety
- Tous les objets sont thread-safe via `Arc` et `RwLock`
- Le téléchargement s'exécute dans un `tokio::spawn` séparé
- Les callbacks de progression utilisent `Arc<dyn Fn>` pour être partagés

### Gestion des erreurs
- Les erreurs sont capturées et stockées dans l'état
- `wait_until_*` retourne l'erreur si elle existe
- Le téléchargement est marqué comme terminé même en cas d'erreur

### Performance
- Téléchargement par chunks (stream)
- Transformation à la volée sans buffer intermédiaire complet (selon le transformer)
- Mise à jour de la progression asynchrone via spawn

## Dépendances

```toml
[dependencies]
reqwest = { version = "0.12", features = ["stream"] }
futures-util = "0.3"
tokio = { version = "1.0", features = ["full"] }

# Optionnel selon les transformers utilisés
async-compression = "0.4"  # Pour GZIP
image = "0.24"              # Pour images
webp = "0.2"                # Pour WebP
```
