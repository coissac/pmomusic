//! Exemple simulant une radio en streaming
//!
//! Cet exemple démontre :
//! - L'utilisation de FifoPlaylist dans un contexte multi-thread
//! - La simulation d'un flux radio continu
//! - La surveillance des changements via update_id
//!
//! Pour exécuter :
//! ```bash
//! cargo run -p pmoplaylist --example radio_streaming
//! ```

use pmoplaylist::{DEFAULT_IMAGE, FifoPlaylist, Track};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    println!("=== Simulation Radio en Streaming ===\n");

    // Créer une radio avec historique limité à 10 tracks
    let radio = FifoPlaylist::new(
        "radio-paradise".to_string(),
        "Radio Paradise - Main Mix".to_string(),
        10,
        DEFAULT_IMAGE,
    );

    println!("📻 Radio créée: {}", radio.title().await);
    println!("📊 Capacité: 10 tracks (historique limité)");
    println!("🆔 ID: {}\n", radio.id().await);

    // Cloner pour les différentes tâches
    let radio_streamer = radio.clone();
    let radio_monitor = radio.clone();
    let radio_client = radio.clone();

    // Tâche 1: Simuler le streaming (ajoute des tracks régulièrement)
    let streamer = tokio::spawn(async move {
        println!("🎵 [STREAMER] Démarrage du flux radio...\n");

        let tracks_data = vec![
            ("Radiohead", "Paranoid Android", "OK Computer", 383),
            ("Massive Attack", "Teardrop", "Mezzanine", 329),
            (
                "Pink Floyd",
                "Shine On You Crazy Diamond",
                "Wish You Were Here",
                810,
            ),
            ("Portishead", "Glory Box", "Dummy", 305),
            ("Dire Straits", "Sultans of Swing", "Dire Straits", 349),
            ("The Cure", "Pictures of You", "Disintegration", 428),
            ("David Bowie", "Heroes", "Heroes", 371),
            (
                "Talking Heads",
                "Once in a Lifetime",
                "Remain in Light",
                259,
            ),
            ("Fleetwood Mac", "Dreams", "Rumours", 257),
            (
                "The Smiths",
                "There Is a Light That Never Goes Out",
                "The Queen Is Dead",
                244,
            ),
            ("Joy Division", "Love Will Tear Us Apart", "Closer", 206),
            ("New Order", "Blue Monday", "Power, Corruption & Lies", 448),
            ("Depeche Mode", "Enjoy the Silence", "Violator", 376),
            ("R.E.M.", "Losing My Religion", "Out of Time", 269),
            (
                "U2",
                "Where the Streets Have No Name",
                "The Joshua Tree",
                337,
            ),
        ];

        for (idx, (artist, title, album, duration)) in tracks_data.iter().enumerate() {
            let track = Track::new(
                format!("radio-track-{}", idx),
                *title,
                format!("http://stream.radioparadise.com/track/{}", idx),
            )
            .with_artist(*artist)
            .with_album(*album)
            .with_duration(*duration);

            radio_streamer.append_track(track).await;

            println!("🎵 [STREAMER] Now Playing: {} - {}", artist, title);

            // Simuler l'attente entre les tracks
            sleep(Duration::from_millis(500)).await;
        }

        println!("\n🎵 [STREAMER] Fin du streaming");
    });

    // Tâche 2: Monitorer les changements (update_id)
    let monitor = tokio::spawn(async move {
        sleep(Duration::from_millis(100)).await;

        println!("👁️  [MONITOR] Surveillance des changements...\n");

        let mut last_update_id = 0;
        let mut iterations = 0;

        loop {
            let current_update_id = radio_monitor.update_id().await;
            let count = radio_monitor.len().await;

            if current_update_id != last_update_id {
                println!(
                    "👁️  [MONITOR] Changement détecté! Update ID: {} → {} | Tracks: {}",
                    last_update_id, current_update_id, count
                );
                last_update_id = current_update_id;
            }

            iterations += 1;
            if iterations >= 50 {
                break;
            }

            sleep(Duration::from_millis(200)).await;
        }

        println!("\n👁️  [MONITOR] Fin de la surveillance");
    });

    // Tâche 3: Client consultant l'historique
    let client = tokio::spawn(async move {
        sleep(Duration::from_millis(2000)).await;

        println!("\n📱 [CLIENT] Consultation de l'historique de la radio...\n");

        // Consulter plusieurs fois pendant le streaming
        for i in 0..3 {
            sleep(Duration::from_millis(2000)).await;

            let history = radio_client.get_items(0, 10).await;
            let update_id = radio_client.update_id().await;

            println!(
                "📱 [CLIENT] Consultation #{} (Update ID: {})",
                i + 1,
                update_id
            );
            println!("   Historique actuel ({} tracks):", history.len());

            for (idx, track) in history.iter().enumerate() {
                let artist = track.artist.as_deref().unwrap_or("Unknown");
                println!("     {}. {} - {}", idx + 1, artist, track.title);
            }
            println!();
        }

        // Générer le container DIDL-Lite à la fin
        println!("📱 [CLIENT] Génération du Container DIDL-Lite...");
        let container = radio_client.as_container().await;
        println!("   Container ID: {}", container.id);
        println!("   Title: {}", container.title);
        println!(
            "   Child Count: {}",
            container.child_count.unwrap_or_default()
        );

        println!("\n📱 [CLIENT] Fin de la consultation");
    });

    // Attendre que toutes les tâches se terminent
    let _ = tokio::join!(streamer, monitor, client);

    // Afficher l'état final
    println!("\n=== État Final ===");
    println!("📊 Total tracks dans la radio: {}", radio.len().await);
    println!("🆔 Update ID final: {}", radio.update_id().await);

    let final_history = radio.get_items(0, 10).await;
    println!("\n🎵 Historique final (10 derniers tracks):");
    for (idx, track) in final_history.iter().enumerate() {
        let artist = track.artist.as_deref().unwrap_or("Unknown");
        let duration_min = track.duration.map(|d| d / 60).unwrap_or(0);
        let duration_sec = track.duration.map(|d| d % 60).unwrap_or(0);
        println!(
            "  {}. {} - {} ({}:{:02})",
            idx + 1,
            artist,
            track.title,
            duration_min,
            duration_sec
        );
    }

    println!("\n=== Simulation terminée ===");
}
