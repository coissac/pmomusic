// Exemple d'utilisation du module download avec transformations

use futures_util::StreamExt;
use pmocache::download::{download_with_transformer, StreamTransformer};
use tokio::io::AsyncWriteExt;

/// Exemple de transformer qui compresse les données en gzip
///
/// Note: Cette fonction nécessite la dépendance `async-compression`
/// Pour l'utiliser, ajoutez à Cargo.toml:
/// ```toml
/// [dev-dependencies]
/// async-compression = { version = "0.4", features = ["tokio", "gzip"] }
/// ```
#[allow(dead_code)]
fn create_gzip_transformer() -> StreamTransformer {
    // Commenté car nécessite async-compression
    // Décommentez si vous ajoutez la dépendance
    unimplemented!("Cette fonction nécessite la dépendance async-compression")

    /*
    Box::new(|response, mut file, update_progress| {
        Box::pin(async move {
            use async_compression::tokio::write::GzipEncoder;

            let mut encoder = GzipEncoder::new(&mut file);
            let mut stream = response.bytes_stream();
            let mut total_written = 0u64;

            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.map_err(|e| format!("Failed to read chunk: {}", e))?;

                encoder
                    .write_all(&chunk)
                    .await
                    .map_err(|e| format!("Failed to write compressed data: {}", e))?;

                total_written += chunk.len() as u64;
                update_progress(total_written);
            }

            encoder
                .shutdown()
                .await
                .map_err(|e| format!("Failed to finalize compression: {}", e))?;

            Ok(())
        })
    })
    */
}

/// Exemple de transformer qui convertit les données en majuscules (exemple simple)
fn create_uppercase_transformer() -> StreamTransformer {
    Box::new(|response, mut file, update_progress| {
        Box::pin(async move {
            let mut stream = response.bytes_stream();
            let mut total_written = 0u64;

            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.map_err(|e| format!("Failed to read chunk: {}", e))?;

                // Transformer en majuscules (seulement pour texte ASCII)
                let transformed: Vec<u8> = chunk
                    .iter()
                    .map(|&b| {
                        if b.is_ascii_lowercase() {
                            b.to_ascii_uppercase()
                        } else {
                            b
                        }
                    })
                    .collect();

                file.write_all(&transformed)
                    .await
                    .map_err(|e| format!("Failed to write: {}", e))?;

                total_written += transformed.len() as u64;
                update_progress(total_written);
            }

            file.flush()
                .await
                .map_err(|e| format!("Failed to flush: {}", e))?;

            Ok(())
        })
    })
}

/// Exemple de transformer qui saute les N premiers bytes (utile pour enlever des headers)
fn create_skip_header_transformer(skip_bytes: usize) -> StreamTransformer {
    Box::new(move |response, mut file, update_progress| {
        Box::pin(async move {
            let mut stream = response.bytes_stream();
            let mut skipped = 0usize;
            let mut total_written = 0u64;

            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.map_err(|e| format!("Failed to read chunk: {}", e))?;

                let to_write = if skipped < skip_bytes {
                    let remaining_to_skip = skip_bytes - skipped;
                    if chunk.len() <= remaining_to_skip {
                        skipped += chunk.len();
                        continue;
                    } else {
                        skipped = skip_bytes;
                        &chunk[remaining_to_skip..]
                    }
                } else {
                    &chunk[..]
                };

                file.write_all(to_write)
                    .await
                    .map_err(|e| format!("Failed to write: {}", e))?;

                total_written += to_write.len() as u64;
                update_progress(total_written);
            }

            file.flush()
                .await
                .map_err(|e| format!("Failed to flush: {}", e))?;

            Ok(())
        })
    })
}

/// Exemple de transformer qui compte les lignes et ajoute des numéros
fn create_line_number_transformer() -> StreamTransformer {
    Box::new(|response, mut file, update_progress| {
        Box::pin(async move {
            let mut stream = response.bytes_stream();
            let mut line_number = 1u32;
            let mut buffer = Vec::new();
            let mut total_written = 0u64;

            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.map_err(|e| format!("Failed to read chunk: {}", e))?;
                buffer.extend_from_slice(&chunk);

                // Traiter les lignes complètes dans le buffer
                while let Some(newline_pos) = buffer.iter().position(|&b| b == b'\n') {
                    let line = &buffer[..newline_pos];

                    // Écrire le numéro de ligne et la ligne
                    let numbered_line = format!("{:6}: ", line_number);
                    file.write_all(numbered_line.as_bytes())
                        .await
                        .map_err(|e| format!("Failed to write: {}", e))?;

                    file.write_all(line)
                        .await
                        .map_err(|e| format!("Failed to write: {}", e))?;

                    file.write_all(b"\n")
                        .await
                        .map_err(|e| format!("Failed to write: {}", e))?;

                    total_written += numbered_line.len() as u64 + line.len() as u64 + 1;
                    update_progress(total_written);

                    line_number += 1;
                    buffer.drain(..=newline_pos);
                }
            }

            // Traiter la dernière ligne si elle n'a pas de newline
            if !buffer.is_empty() {
                let numbered_line = format!("{:6}: ", line_number);
                file.write_all(numbered_line.as_bytes())
                    .await
                    .map_err(|e| format!("Failed to write: {}", e))?;

                file.write_all(&buffer)
                    .await
                    .map_err(|e| format!("Failed to write: {}", e))?;

                total_written += numbered_line.len() as u64 + buffer.len() as u64;
                update_progress(total_written);
            }

            file.flush()
                .await
                .map_err(|e| format!("Failed to flush: {}", e))?;

            Ok(())
        })
    })
}

#[tokio::main]
async fn main() {
    println!("=== Exemples de transformers pour le module download ===\n");

    let temp_dir = std::env::temp_dir();

    // Exemple 1: Téléchargement avec transformation en majuscules
    println!("1. Téléchargement avec transformation en MAJUSCULES");
    let uppercase_file = temp_dir.join("uppercase_example.txt");
    let _ = std::fs::remove_file(&uppercase_file);

    let transformer = create_uppercase_transformer();
    let dl = download_with_transformer(
        &uppercase_file,
        "https://www.rust-lang.org/",
        Some(transformer),
    );

    println!("   Téléchargement démarré...");
    match dl.wait_until_finished().await {
        Ok(_) => {
            println!("   ✓ Téléchargement terminé!");
            println!("   - Taille source: {} bytes", dl.current_size().await);
            println!(
                "   - Taille transformée: {} bytes",
                dl.transformed_size().await
            );
        }
        Err(e) => {
            eprintln!("   ✗ Erreur: {}", e);
        }
    }

    // Exemple 2: Skip header
    println!("\n2. Téléchargement en sautant les 100 premiers bytes");
    let skip_file = temp_dir.join("skip_header_example.txt");
    let _ = std::fs::remove_file(&skip_file);

    let transformer = create_skip_header_transformer(100);
    let dl = download_with_transformer(&skip_file, "https://www.rust-lang.org/", Some(transformer));

    match dl.wait_until_finished().await {
        Ok(_) => {
            println!("   ✓ Téléchargement terminé!");
            println!(
                "   - Taille transformée: {} bytes",
                dl.transformed_size().await
            );
        }
        Err(e) => {
            eprintln!("   ✗ Erreur: {}", e);
        }
    }

    // Exemple 3: Numérotation des lignes
    println!("\n3. Téléchargement avec numérotation des lignes");
    let numbered_file = temp_dir.join("numbered_example.txt");
    let _ = std::fs::remove_file(&numbered_file);

    let transformer = create_line_number_transformer();
    let dl = download_with_transformer(
        &numbered_file,
        "https://www.rust-lang.org/",
        Some(transformer),
    );

    match dl.wait_until_finished().await {
        Ok(_) => {
            println!("   ✓ Téléchargement terminé!");
            println!(
                "   - Taille transformée: {} bytes",
                dl.transformed_size().await
            );
        }
        Err(e) => {
            eprintln!("   ✗ Erreur: {}", e);
        }
    }

    println!("\n=== Exemples terminés ===");
    println!("Fichiers créés dans: {:?}", temp_dir);

    // Note: Commenté car nécessite la dépendance async-compression
    /*
    println!("\n4. Téléchargement avec compression GZIP");
    let gzip_file = temp_dir.join("compressed_example.gz");
    let _ = std::fs::remove_file(&gzip_file);

    let transformer = create_gzip_transformer();
    let dl = download_with_transformer(
        &gzip_file,
        "https://www.rust-lang.org/",
        Some(transformer),
    );

    match dl.wait_until_finished().await {
        Ok(_) => {
            println!("   ✓ Téléchargement terminé!");
            println!("   - Taille source: {} bytes", dl.current_size().await);
            println!("   - Taille compressée: {} bytes", dl.transformed_size().await);
            let ratio = 100.0 * dl.transformed_size().await as f64 / dl.current_size().await as f64;
            println!("   - Ratio de compression: {:.1}%", ratio);
        }
        Err(e) => {
            eprintln!("   ✗ Erreur: {}", e);
        }
    }
    */
}
