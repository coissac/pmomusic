//! Exemple d'utilisation de RadioParadiseStreamSource
//!
//! Ce exemple montre comment :
//! - Créer un RadioParadiseStreamSource
//! - Ajouter des block IDs à télécharger via push_block_id()
//! - Connecter à un sink pour récupérer les segments audio

use pmoaudio::{
    nodes::{DEFAULT_CHUNK_DURATION_MS, TypedAudioNode},
    pipeline::AudioPipelineNode,
};
use pmoparadise::{
    client::RadioParadiseClient,
    models::EventId,
    RadioParadiseStreamSource,
};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Créer un client Radio Paradise
    let client = RadioParadiseClient::new(pmoparadise::Channel::MainMix);

    // 2. Créer le source node avec durée de chunk par défaut (500ms)
    let mut source = RadioParadiseStreamSource::new(
        client.clone(),
        DEFAULT_CHUNK_DURATION_MS,
    );

    // 3. Ajouter des blocks IDs à télécharger
    // Dans un cas réel, ces IDs viendraient du nowplaying stream
    source.push_block_id(EventId(12345));
    source.push_block_id(EventId(12346));
    source.push_block_id(EventId(12347));

    // 4. Optionnel : Connecter à un sink (ici juste un exemple de structure)
    // let sink = create_your_sink();
    // source.add_child(Box::new(sink));

    // 5. Lancer le traitement
    let stop_token = CancellationToken::new();

    println!("🎵 RadioParadiseStreamSource lancé...");
    println!("   - Téléchargement et décodage des blocs FLAC");
    println!("   - Insertion automatique des TrackBoundary");
    println!("   - Cache anti-redondance de {} blocs", 10);

    // Dans un cas réel, on lancerait :
    // source.run(stop_token).await?;

    // Pour cet exemple, on simule juste le comportement
    println!("\n✅ Configuration réussie !");
    println!("\nFlux d'exécution :");
    println!("1. Attente d'un block ID dans la queue (timeout 3s)");
    println!("2. Vérification cache anti-redondance");
    println!("3. Téléchargement des métadonnées du bloc");
    println!("4. Téléchargement et décodage du FLAC (bitrate=4)");
    println!("5. Envoi des AudioChunk (I16 ou I24)");
    println!("6. Insertion TrackBoundary au timing correct (basé sur samples)");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Exemple avancé : Utilisation avec nowplaying stream
// ═══════════════════════════════════════════════════════════════════════════

#[allow(dead_code)]
async fn example_with_nowplaying_stream() -> Result<(), Box<dyn std::error::Error>> {
    use futures_util::StreamExt;
    use pmoparadise::Channel;

    let client = RadioParadiseClient::new(Channel::MainMix);
    let mut source = RadioParadiseStreamSource::new(
        client.clone(),
        DEFAULT_CHUNK_DURATION_MS,
    );

    // Récupérer le nowplaying stream
    let nowplaying = client.nowplaying_stream().await?;

    // Clone pour le spawned task
    let stop_token = CancellationToken::new();
    let stop_clone = stop_token.clone();

    // Task 1 : Alimenter la queue avec les nouveaux blocks
    let feed_task = tokio::spawn(async move {
        tokio::pin!(nowplaying);

        while let Some(result) = nowplaying.next().await {
            match result {
                Ok(event) => {
                    println!("📻 Nouveau bloc détecté : {:?}", event.event);
                    source.push_block_id(event.event);
                }
                Err(e) => {
                    eprintln!("❌ Erreur nowplaying stream : {}", e);
                    break;
                }
            }
        }
    });

    // Task 2 : Traiter les blocs (dans un cas réel)
    // let process_task = tokio::spawn(async move {
    //     source.run(stop_clone).await
    // });

    // Attendre les tasks
    feed_task.await?;
    // process_task.await??;

    Ok(())
}
