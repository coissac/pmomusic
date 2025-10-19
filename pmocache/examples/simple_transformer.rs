/// Exemple minimal de transformer sans dépendances externes complexes

// Import direct du type depuis le module
// Note: Cet exemple montre comment utiliser l'API de transformation

fn main() {
    println!("Exemple d'utilisation du module download avec transformers\n");

    println!("1. Téléchargement simple:");
    println!("   let dl = download(\"/tmp/file.dat\", \"https://example.com/file.dat\");");
    println!("   dl.wait_until_finished().await?;\n");

    println!("2. Téléchargement avec transformation:");
    println!(
        "   let transformer: StreamTransformer = Box::new(|response, mut file, update_progress| {{"
    );
    println!("       Box::pin(async move {{");
    println!("           let mut stream = response.bytes_stream();");
    println!("           let mut total = 0u64;");
    println!();
    println!("           while let Some(chunk_result) = stream.next().await {{");
    println!("               let chunk = chunk_result.map_err(|e| e.to_string())?;");
    println!();
    println!("               // Transformation ici (ex: compression, conversion)");
    println!("               let transformed = process(chunk);");
    println!();
    println!("               file.write_all(&transformed).await.map_err(|e| e.to_string())?;");
    println!("               total += transformed.len() as u64;");
    println!("               update_progress(total);");
    println!("           }}");
    println!();
    println!("           file.flush().await.map_err(|e| e.to_string())?;");
    println!("           Ok(())");
    println!("       }})");
    println!("   }});\n");

    println!("   let dl = download_with_transformer(\"/tmp/out.dat\", \"https://example.com/in.dat\", Some(transformer));");
    println!("   dl.wait_until_finished().await?;\n");

    println!("3. Méthodes disponibles sur Download:");
    println!("   - filename()         : Chemin du fichier");
    println!("   - current_size()     : Taille téléchargée (source)");
    println!("   - transformed_size() : Taille transformée (destination)");
    println!("   - expected_size()    : Taille attendue (Content-Length)");
    println!("   - finished()         : Téléchargement terminé?");
    println!("   - error()            : Erreur éventuelle");
    println!("   - wait_until_min_size(n) : Attend au moins n bytes");
    println!("   - wait_until_finished()  : Attend la fin");
    println!("   - open()                 : Ouvre le fichier pour lecture");
    println!("   - pos() / set_pos()      : Position de lecture\n");

    println!("Pour des exemples complets, voir:");
    println!("  - examples/test_download_transformer.rs");
    println!("  - DOWNLOAD_MODULE.md");
}
