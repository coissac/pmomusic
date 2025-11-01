//! Exemples d'utilisation de l'API AudioChunk et AudioSegment
//!
//! Ce fichier démontre les différentes façons de créer et manipuler
//! des chunks audio avec la nouvelle architecture générique.

use pmoaudio::*;

fn main() {
    println!("=== Exemples d'utilisation de l'API AudioChunk ===\n");

    // ============ Création de chunks de différents types ============
    example_create_chunks();

    // ============ Conversions entre types ============
    example_conversions();

    // ============ Utilisation des macros ============
    example_macros();

    // ============ AudioSegment et helpers ============
    example_audio_segments();

    // ============ Manipulation du gain ============
    example_gain_manipulation();
}

fn example_create_chunks() {
    println!(">>> Création de chunks audio\n");

    // Chunk I32 stéréo
    let stereo_i32 = vec![[1000i32, 2000i32], [3000i32, 4000i32]];
    let chunk_i32 = AudioChunkData::new(stereo_i32, 48000, 0.0);
    println!(
        "Chunk I32: {} frames @ {}Hz",
        chunk_i32.len(),
        chunk_i32.sample_rate()
    );

    // Chunk F32 stéréo (normalisé [-1.0, 1.0])
    let stereo_f32 = vec![[0.5f32, -0.5f32], [0.8f32, -0.8f32]];
    let chunk_f32 = AudioChunkData::new(stereo_f32, 48000, 0.0);
    println!(
        "Chunk F32: {} frames @ {}Hz",
        chunk_f32.len(),
        chunk_f32.sample_rate()
    );

    // Chunk depuis canaux séparés
    let left = vec![100i32, 200i32, 300i32];
    let right = vec![150i32, 250i32, 350i32];
    let chunk_from_channels = AudioChunkData::<i32>::from_channels(left, right, 44100);
    println!("Chunk from channels: {} frames", chunk_from_channels.len());

    // Chunk avec gain
    let chunk_with_gain = AudioChunkData::new(
        vec![[1000i32, 2000i32]],
        48000,
        6.0, // +6 dB
    );
    println!("Chunk with gain: {} dB\n", chunk_with_gain.gain_db());
}

fn example_conversions() {
    println!(">>> Conversions entre types\n");

    // Créer un chunk I32
    let i32_data = vec![[1_000_000i32, 2_000_000i32]];
    let chunk_i32 = AudioChunkData::new(i32_data, 48000, 0.0);
    let audio_chunk = AudioChunk::I32(chunk_i32);

    println!("Type original: {}", audio_chunk.type_name());

    // Conversion vers F32
    let audio_chunk_f32 = audio_chunk.to_f32();
    println!("Après conversion to_f32: {}", audio_chunk_f32.type_name());

    // Conversion vers F64
    let audio_chunk_f64 = audio_chunk_f32.to_f64();
    println!("Après conversion to_f64: {}", audio_chunk_f64.type_name());

    // Retour vers I32
    let audio_chunk_back = audio_chunk_f64.to_i32();
    println!("Après conversion to_i32: {}", audio_chunk_back.type_name());

    // Utilisation des traits From/Into
    let chunk_i16 = AudioChunkData::new(vec![[1000i16, 2000i16]], 48000, 0.0);
    let chunk_i32_from_i16: std::sync::Arc<AudioChunkData<i32>> = (&*chunk_i16).into();
    println!(
        "\nConversion I16 → I32 via Into: {} frames",
        chunk_i32_from_i16.len()
    );

    println!();
}

fn example_macros() {
    println!(">>> Utilisation des macros\n");

    // Créer différents types de chunks
    let chunk_i32 = AudioChunk::I32(AudioChunkData::new(vec![[100i32, 200i32]], 48000, 0.0));
    let chunk_f32 = AudioChunk::F32(AudioChunkData::new(vec![[0.5f32, -0.5f32]], 48000, 0.0));

    // Macro is_chunk_type!
    println!("chunk_i32 is I32: {}", is_chunk_type!(&chunk_i32, I32));
    println!("chunk_i32 is F32: {}", is_chunk_type!(&chunk_i32, F32));
    println!("chunk_f32 is F32: {}", is_chunk_type!(&chunk_f32, F32));

    // Macro extract_chunk_data!
    if let Some(data) = extract_chunk_data!(&chunk_i32, I32) {
        println!("\nExtracted I32 data: {} frames", data.len());
    }

    // Macro match_chunk! pour traiter n'importe quel type
    let frame_count = match_chunk!(&chunk_i32, data => {
        data.len()
    });
    println!("Frame count via match_chunk: {}", frame_count);

    // Macro map_chunk! pour transformer tout en préservant le type
    let chunk_with_gain = map_chunk!(&chunk_i32, data => {
        data.set_gain_db(6.0)
    });
    println!("\nGain après map_chunk: {} dB", chunk_with_gain.gain_db());

    println!();
}

fn example_audio_segments() {
    println!(">>> AudioSegment et helpers\n");

    // Créer un segment audio
    let segment = AudioSegment::new_chunk(
        0,
        0.0,
        vec![[1000i32, 2000i32], [3000i32, 4000i32]],
        48000,
        BitDepth::B32,
    );

    // Accès aux propriétés via les helpers
    println!("Segment info:");
    println!("  - Type: {}", segment.chunk_type_name().unwrap());
    println!("  - Sample rate: {} Hz", segment.sample_rate().unwrap());
    println!("  - Frame count: {}", segment.frame_count().unwrap());
    println!("  - Gain: {} dB", segment.gain_db().unwrap());

    // Conversion du chunk
    if let Some(f32_chunk) = segment.to_f32_chunk() {
        println!("\nChunk converti en F32: {}", f32_chunk.type_name());
    }

    // Créer un marqueur de sync
    let heartbeat = AudioSegment::new_hearbeat(1, 1.0);
    println!("\nHeartbeat segment:");
    println!("  - Is audio: {}", heartbeat.is_audio_chunk());
    println!("  - Is heartbeat: {}", heartbeat.is_heartbeat());

    // Macro extract_audio_chunk!
    if let Some(chunk) = extract_audio_chunk!(&*segment) {
        println!("\nExtracted chunk type: {}", chunk.type_name());
    }

    // Macro match_segment!
    let info = match_segment!(&*segment,
        chunk => format!("Audio chunk: {}", chunk.type_name()),
        _marker => "Sync marker".to_string()
    );
    println!("Segment info via macro: {}", info);

    println!();
}

fn example_gain_manipulation() {
    println!(">>> Manipulation du gain\n");

    // Créer un segment
    let segment = AudioSegment::new_chunk(0, 0.0, vec![[1000i32, 2000i32]], 48000, BitDepth::B32);

    println!("Gain initial: {} dB", segment.gain_db().unwrap());

    // Définir un gain absolu
    let segment_6db = segment.with_gain_db(6.0).unwrap();
    println!(
        "Après with_gain_db(6.0): {} dB",
        segment_6db.gain_db().unwrap()
    );

    // Ajuster le gain (relatif)
    let segment_9db = segment_6db.adjust_gain_db(3.0).unwrap();
    println!(
        "Après adjust_gain_db(+3.0): {} dB",
        segment_9db.gain_db().unwrap()
    );

    // Les segments originaux ne sont pas modifiés (immutabilité)
    println!(
        "Gain du segment original: {} dB",
        segment.gain_db().unwrap()
    );

    // Conversion gain linéaire ↔ dB
    let linear_gain = gain_linear_from_db(6.0);
    let gain_db = gain_db_from_linear(linear_gain);
    println!("\n6 dB = {:.4}x (linéaire)", linear_gain);
    println!("{:.4}x = {:.2} dB", linear_gain, gain_db);

    println!();
}
