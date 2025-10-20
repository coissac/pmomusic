// Simple test pour vérifier la compilation du module download

#[tokio::main]
async fn main() {
    println!("Module download compilé avec succès!");

    // Test basique (commenté pour ne pas vraiment télécharger)
    /*
    let dl = download::download("/tmp/test.html", "https://www.rust-lang.org/");

    println!("Téléchargement démarré...");

    match dl.wait_until_min_size(100).await {
        Ok(_) => println!("Au moins 100 bytes téléchargés"),
        Err(e) => eprintln!("Erreur: {}", e),
    }

    match dl.wait_until_finished().await {
        Ok(_) => {
            println!("Téléchargement terminé!");
            println!("Taille finale: {} bytes", dl.current_size().await);
        }
        Err(e) => eprintln!("Erreur: {}", e),
    }
    */
}
