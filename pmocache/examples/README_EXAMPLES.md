# Exemples du module Download

Ce répertoire contient des exemples d'utilisation du module `download` de pmocache.

## Fichiers

### `test_download.rs`
Exemple basique de téléchargement sans transformation.

**Utilisation:**
```bash
cargo run --example test_download
```

### `test_download_transformer.rs`
Exemples complets de transformers :
- Transformation en majuscules
- Suppression de header (skip N bytes)
- Numérotation des lignes
- Compression GZIP (commenté, nécessite async-compression)

**Utilisation:**
```bash
cargo run --example test_download_transformer
```

### `simple_transformer.rs`
Exemple de documentation montrant la syntaxe et l'API.

**Utilisation:**
```bash
cargo run --example simple_transformer
```

## Concepts clés

### 1. Téléchargement simple

```rust
use pmocache::download::download;

let dl = download("/tmp/file.dat", "https://example.com/file.dat");
dl.wait_until_finished().await?;
```

### 2. Téléchargement avec transformer

Un transformer est une fonction qui :
1. Reçoit le stream de réponse HTTP
2. Reçoit un fichier ouvert en écriture
3. Reçoit un callback de progression
4. Traite les données à la volée
5. Écrit le résultat transformé dans le fichier

```rust
let transformer: StreamTransformer = Box::new(|response, mut file, update_progress| {
    Box::pin(async move {
        let mut stream = response.bytes_stream();
        let mut total = 0u64;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| e.to_string())?;

            // Transformer les données
            let transformed = your_transformation(&chunk);

            // Écrire le résultat
            file.write_all(&transformed).await.map_err(|e| e.to_string())?;

            // Mettre à jour la progression
            total += transformed.len() as u64;
            update_progress(total);
        }

        file.flush().await.map_err(|e| e.to_string())?;
        Ok(())
    })
});

let dl = download_with_transformer("/tmp/output.dat", "https://example.com/input.dat", Some(transformer));
```

### 3. Suivi de progression

```rust
let dl = download("/tmp/file.dat", "https://example.com/file.dat");

// Attendre au moins 1MB
dl.wait_until_min_size(1024 * 1024).await?;
println!("Au moins 1MB téléchargés");

// Voir la progression
loop {
    let current = dl.current_size().await;
    let expected = dl.expected_size().await;

    if let Some(total) = expected {
        println!("Progression: {}/{} bytes ({:.1}%)",
            current, total, 100.0 * current as f64 / total as f64);
    } else {
        println!("Téléchargés: {} bytes", current);
    }

    if dl.finished().await {
        break;
    }

    tokio::time::sleep(Duration::from_millis(100)).await;
}
```

## Cas d'usage pour PMOMusic

### Conversion d'images pour le cache

```rust
// Télécharger une couverture d'album et la convertir en WebP
fn webp_transformer() -> StreamTransformer {
    Box::new(|response, mut file, update_progress| {
        Box::pin(async move {
            let bytes = response.bytes().await.map_err(|e| e.to_string())?;
            let img = image::load_from_memory(&bytes)
                .map_err(|e| format!("Decode error: {}", e))?;

            let encoder = webp::Encoder::from_image(&img)
                .map_err(|e| format!("Encode error: {}", e))?;
            let webp = encoder.encode(75.0);

            file.write_all(&*webp).await.map_err(|e| e.to_string())?;
            file.flush().await.map_err(|e| e.to_string())?;

            update_progress(webp.len() as u64);
            Ok(())
        })
    })
}

// Utilisation dans pmocovers
let transformer = webp_transformer();
let dl = download_with_transformer(cache_path, cover_url, Some(transformer));
```

### Conversion audio pour le cache

```rust
// Télécharger du MP3 et le convertir en FLAC
fn mp3_to_flac_transformer() -> StreamTransformer {
    Box::new(|response, mut file, update_progress| {
        Box::pin(async move {
            let mp3_bytes = response.bytes().await.map_err(|e| e.to_string())?;

            // Décoder MP3
            let decoded = decode_mp3(&mp3_bytes)?;

            // Encoder FLAC
            let flac_bytes = encode_flac(&decoded)?;

            file.write_all(&flac_bytes).await.map_err(|e| e.to_string())?;
            file.flush().await.map_err(|e| e.to_string())?;

            update_progress(flac_bytes.len() as u64);
            Ok(())
        })
    })
}

// Utilisation dans pmoaudiocache
let transformer = mp3_to_flac_transformer();
let dl = download_with_transformer(cache_path, audio_url, Some(transformer));
```

### Streaming progressif

```rust
// Commencer à lire pendant le téléchargement
let dl = download(audio_path, stream_url);

// Attendre le buffer minimal (256KB)
dl.wait_until_min_size(256 * 1024).await?;

// Ouvrir et commencer à lire
let mut file = dl.open()?;
let mut buffer = [0u8; 4096];

loop {
    // Lire ce qui est disponible
    match file.read(&mut buffer) {
        Ok(0) if dl.finished().await => break, // EOF
        Ok(0) => {
            // Pas encore de données, attendre un peu
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(n) => {
            // Traiter les données lues
            process_audio_chunk(&buffer[..n]);
        }
        Err(e) => return Err(e.into()),
    }
}
```

## Voir aussi

- [DOWNLOAD_MODULE.md](../DOWNLOAD_MODULE.md) - Documentation complète du module
- [src/download.rs](../src/download.rs) - Code source
