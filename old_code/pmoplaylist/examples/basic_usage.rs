//! Exemple d'utilisation basique de pmoplaylist
//!
//! Pour exécuter cet exemple :
//! ```bash
//! cargo run -p pmoplaylist --example basic_usage
//! ```

use pmoplaylist::{FifoPlaylist, Track, DEFAULT_IMAGE};

#[tokio::main]
async fn main() {
    println!("=== Exemple pmoplaylist ===\n");

    // 1. Créer une playlist FIFO
    println!("1. Création d'une playlist avec capacité de 5 tracks...");
    let playlist = FifoPlaylist::new(
        "my-radio".to_string(),
        "Ma Radio Préférée".to_string(),
        5,
        DEFAULT_IMAGE,
    );
    println!("   ✓ Playlist créée: {}", playlist.title().await);
    println!("   ✓ ID: {}", playlist.id().await);
    println!("   ✓ Capacité: 5 tracks");
    println!("   ✓ Update ID initial: {}\n", playlist.update_id().await);

    // 2. Ajouter des tracks
    println!("2. Ajout de 3 tracks...");
    let tracks = vec![
        Track::new(
            "track-1",
            "Bohemian Rhapsody",
            "http://example.com/queen/bohemian.flac",
        )
        .with_artist("Queen")
        .with_album("A Night at the Opera")
        .with_duration(354)
        .with_image("http://example.com/covers/queen-anato.jpg"),
        Track::new(
            "track-2",
            "Stairway to Heaven",
            "http://example.com/zeppelin/stairway.mp3",
        )
        .with_artist("Led Zeppelin")
        .with_album("Led Zeppelin IV")
        .with_duration(482),
        Track::new(
            "track-3",
            "Hotel California",
            "http://example.com/eagles/hotel.flac",
        )
        .with_artist("Eagles")
        .with_album("Hotel California")
        .with_duration(391),
    ];

    for track in tracks {
        playlist.append_track(track.clone()).await;
        println!(
            "   ✓ Ajouté: {} - {}",
            track.title,
            track.artist.unwrap_or_default()
        );
    }

    println!("\n   Total tracks: {}", playlist.len().await);
    println!("   Update ID: {}\n", playlist.update_id().await);

    // 3. Tester le comportement FIFO
    println!("3. Test du comportement FIFO (capacité = 5)...");
    println!("   Ajout de 4 tracks supplémentaires...");

    for i in 4..=7 {
        let track = Track::new(
            format!("track-{}", i),
            format!("Song Number {}", i),
            format!("http://example.com/songs/{}.mp3", i),
        );
        playlist.append_track(track).await;
    }

    println!(
        "   ✓ Total tracks (limité par capacité): {}",
        playlist.len().await
    );

    // Afficher les tracks actuels
    let items = playlist.get_items(0, 10).await;
    println!("\n   Tracks actuels dans la FIFO:");
    for (idx, track) in items.iter().enumerate() {
        println!("     {}. {} ({})", idx + 1, track.title, track.id);
    }
    println!("   (Les tracks 1 et 2 ont été supprimés automatiquement)\n");

    // 4. Supprimer le plus ancien
    println!("4. Suppression du track le plus ancien...");
    if let Some(removed) = playlist.remove_oldest().await {
        println!("   ✓ Supprimé: {} ({})", removed.title, removed.id);
    }
    println!("   Total tracks: {}", playlist.len().await);
    println!("   Update ID: {}\n", playlist.update_id().await);

    // 5. Supprimer par ID
    println!("5. Suppression d'un track par ID (track-5)...");
    if playlist.remove_by_id("track-5").await {
        println!("   ✓ Track supprimé");
    }
    println!("   Total tracks: {}", playlist.len().await);
    println!("   Update ID: {}\n", playlist.update_id().await);

    // 6. Générer un Container DIDL-Lite
    println!("6. Génération du Container DIDL-Lite...");
    let container = playlist.as_container().await;
    println!("   Container:");
    println!("     - ID: {}", container.id);
    println!("     - Parent ID: {}", container.parent_id);
    println!("     - Title: {}", container.title);
    println!("     - Class: {}", container.class);
    println!(
        "     - Child Count: {}\n",
        container.child_count.unwrap_or_default()
    );

    // 7. Générer des Items DIDL-Lite
    println!("7. Génération des Items DIDL-Lite...");
    let didl_items = playlist
        .as_objects(0, 10, Some("http://myserver/api/default-image"))
        .await;

    println!("   Items DIDL-Lite:");
    for (idx, item) in didl_items.iter().enumerate() {
        println!("\n   Item {}:", idx + 1);
        println!("     - ID: {}", item.id);
        println!("     - Title: {}", item.title);
        println!("     - Artist: {}", item.artist.as_deref().unwrap_or("N/A"));
        println!("     - Album: {}", item.album.as_deref().unwrap_or("N/A"));
        println!("     - Class: {}", item.class);
        println!("     - Parent ID: {}", item.parent_id);

        if !item.resources.is_empty() {
            println!("     - Resource URI: {}", item.resources[0].url);
            if let Some(ref duration) = item.resources[0].duration {
                println!("     - Duration: {}", duration);
            }
        }

        if let Some(ref art) = item.album_art {
            println!("     - Album Art: {}", art);
        }
    }

    // 8. Image par défaut
    println!("\n8. Image par défaut...");
    let default_image = playlist.default_image().await;
    println!(
        "   ✓ Taille de l'image par défaut: {} bytes",
        default_image.len()
    );
    println!("   (Cette image peut être servie via un endpoint HTTP)\n");

    // 9. Vider la playlist
    println!("9. Vidage de la playlist...");
    playlist.clear().await;
    println!("   ✓ Playlist vidée");
    println!("   Total tracks: {}", playlist.len().await);
    println!("   Is empty: {}", playlist.is_empty().await);
    println!("   Update ID final: {}\n", playlist.update_id().await);

    println!("=== Exemple terminé ===");
}
