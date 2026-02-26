//! AAC Decoder Module (ADTS streaming)
//!
//! Provides asynchronous streaming AAC/ADTS decoding via libfdk-aac (statically linked).
//! Decodes AAC audio streams into PCM data (16-bit little-endian interleaved).
//!
//! Designed for live radio streams (e.g. Radio France icecast AAC 192kbps).
//! No seek required — pure linear streaming.
//!
//! ## Architecture
//!
//! ```text
//! AAC Input → [Ingest Task] → [Decode Task (blocking)] → [Writer Task] → PCM Output (AsyncRead)
//! ```

use fdk_aac::dec::{Decoder, DecoderError, Transport};
use tokio::{
    io::AsyncRead,
    sync::{mpsc, oneshot},
};

use crate::{
    common::ChannelReader,
    decoder_common::{
        spawn_ingest_task, spawn_writer_task, DecodedStream, DecoderError as PmoDecoderError,
        CHANNEL_CAPACITY, DUPLEX_BUFFER_SIZE,
    },
    pcm::StreamInfo,
    stream::ManagedAsyncReader,
};

/// Errors that can occur while decoding AAC data.
pub type AacError = PmoDecoderError;

/// Async decoded AAC stream.
pub type AacDecodedStream = DecodedStream<AacError>;

/// Decodes an AAC/ADTS stream into PCM audio data (16-bit little-endian interleaved).
///
/// The input must be a raw ADTS stream (as produced by Radio France icecast).
/// No seek is required — the decoder processes frames linearly.
pub async fn decode_aac_stream<R>(reader: R) -> Result<AacDecodedStream, AacError>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let (ingest_tx, ingest_rx) = mpsc::channel(CHANNEL_CAPACITY);
    spawn_ingest_task(reader, ingest_tx);

    let (pcm_tx, pcm_rx) = mpsc::channel(CHANNEL_CAPACITY);
    let (pcm_reader, pcm_writer) = tokio::io::duplex(DUPLEX_BUFFER_SIZE);
    let (info_tx, info_rx) = oneshot::channel::<Result<StreamInfo, AacError>>();

    let blocking_handle = tokio::task::spawn_blocking(move || -> Result<(), AacError> {
        let mut channel_reader = ChannelReader::<AacError>::new(ingest_rx);
        let mut decoder = Decoder::new(Transport::Adts);
        let mut info_tx = Some(info_tx);

        // Buffer de lecture — on lit des chunks et on les pousse au décodeur
        let mut read_buf = vec![0u8; 8192];
        // Buffer de sortie PCM — fdk-aac écrit des frames entières
        let mut pcm_out = vec![0i16; 8192];

        use std::io::Read;

        loop {
            // Lire des bytes depuis le stream ADTS
            let n = match channel_reader.read(&mut read_buf) {
                Ok(0) => break, // EOF
                Ok(n) => n,
                Err(e) => {
                    let err = AacError::Io {
                        kind: e.kind(),
                        message: e.to_string(),
                    };
                    if let Some(tx) = info_tx.take() {
                        let _ = tx.send(Err(err.clone()));
                    }
                    return Err(err);
                }
            };

            // Pousser les bytes au décodeur fdk-aac
            let filled = match decoder.fill(&read_buf[..n]) {
                Ok(filled) => filled,
                Err(e) => {
                    let err = AacError::Decode(format!("fdk-aac fill error: {:?}", e));
                    if let Some(tx) = info_tx.take() {
                        let _ = tx.send(Err(err.clone()));
                    }
                    return Err(err);
                }
            };

            // Si fill n'a pas consommé tous les bytes, on les remet devant
            // (fdk-aac peut ne pas consommer tout en une passe)
            // Note: fdk-aac retourne le nombre de bytes non-consommés — on les ignore
            // car le décodeur conserve son état interne entre appels.
            let _ = filled;

            // Décoder les frames disponibles
            loop {
                // Adapter la taille du buffer PCM si nécessaire
                let stream_info = decoder.stream_info();
                let frame_size = if stream_info.frameSize > 0 && stream_info.numChannels > 0 {
                    (stream_info.frameSize * stream_info.numChannels) as usize
                } else {
                    2048 // taille par défaut avant la première frame
                };

                if pcm_out.len() < frame_size {
                    pcm_out.resize(frame_size, 0i16);
                }

                match decoder.decode_frame(&mut pcm_out) {
                    Ok(()) => {
                        let info_ref = decoder.stream_info();

                        // Envoyer les infos au premier décodage réussi
                        if let Some(tx) = info_tx.take() {
                            let info = StreamInfo {
                                sample_rate: info_ref.sampleRate as u32,
                                channels: info_ref.numChannels as u8,
                                bits_per_sample: 16,
                                total_samples: None,
                                max_block_size: 0,
                                min_block_size: 0,
                            };
                            if tx.send(Ok(info)).is_err() {
                                return Ok(());
                            }
                        }

                        // Convertir i16 → bytes little-endian
                        let samples = &pcm_out[..frame_size];
                        let mut bytes = Vec::with_capacity(samples.len() * 2);
                        for s in samples {
                            bytes.extend_from_slice(&s.to_le_bytes());
                        }

                        if pcm_tx.blocking_send(Ok(bytes)).is_err() {
                            return Ok(());
                        }
                    }
                    Err(DecoderError::NOT_ENOUGH_BITS) => {
                        // Pas assez de données — besoin de plus d'input
                        break;
                    }
                    Err(DecoderError::TRANSPORT_SYNC_ERROR) => {
                        // Erreur de sync ADTS transitoire — continuer
                        break;
                    }
                    Err(e) => {
                        let err = AacError::Decode(format!("fdk-aac decode error: {:?}", e));
                        if let Some(tx) = info_tx.take() {
                            let _ = tx.send(Err(err.clone()));
                        }
                        return Err(err);
                    }
                }
            }
        }

        if let Some(tx) = info_tx.take() {
            let err = AacError::Decode("AAC stream contained no decodable frames".into());
            let _ = tx.send(Err(err.clone()));
            return Err(err);
        }

        Ok(())
    });

    let writer_handle = spawn_writer_task(pcm_rx, pcm_writer, blocking_handle, "aac-decode");

    let info = info_rx.await.map_err(|_| AacError::ChannelClosed)??;
    let reader = ManagedAsyncReader::new("aac-decode-writer", pcm_reader, writer_handle);

    Ok(DecodedStream::new(info, reader))
}
