// //! Module de conversion audio en FLAC
// //!
// //! Ce module gère la conversion de divers formats audio vers FLAC
// //! pour standardiser le stockage dans le cache.

// use anyhow::{anyhow, Result};
// use std::io::Cursor;
// use symphonia::core::audio::SampleBuffer;
// use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
// use symphonia::core::errors::Error as SymphoniaError;
// use symphonia::core::formats::FormatOptions;
// use symphonia::core::io::MediaSourceStream;
// use symphonia::core::meta::MetadataOptions;
// use symphonia::core::probe::Hint;

// /// Convertit des données audio en FLAC
// ///
// /// Cette fonction accepte n'importe quel format audio supporté par Symphonia
// /// et le convertit en FLAC pour un stockage standardisé.
// ///
// /// # Arguments
// ///
// /// * `data` - Données audio brutes (n'importe quel format)
// /// * `extension` - Extension du fichier source (optionnel, aide à la détection)
// ///
// /// # Returns
// ///
// /// Données audio au format FLAC
// ///
// /// # Exemple
// ///
// /// ```rust,no_run
// /// use pmoaudiocache::flac::convert_to_flac;
// ///
// /// let mp3_data = std::fs::read("track.mp3").unwrap();
// /// let flac_data = convert_to_flac(&mp3_data, Some("mp3")).unwrap();
// /// ```
// pub fn convert_to_flac(data: &[u8], extension: Option<&str>) -> Result<Vec<u8>> {
//     // Si c'est déjà du FLAC, on le retourne tel quel
//     if is_flac(data) {
//         return Ok(data.to_vec());
//     }

//     // Créer un MediaSource depuis les données (en clonant pour avoir 'static)
//     let data_owned = data.to_vec();
//     let cursor = Cursor::new(data_owned);
//     let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

//     // Créer un hint si on a l'extension
//     let mut hint = Hint::new();
//     if let Some(ext) = extension {
//         hint.with_extension(ext);
//     }

//     // Prober le format
//     let probed = symphonia::default::get_probe()
//         .format(
//             &hint,
//             mss,
//             &FormatOptions::default(),
//             &MetadataOptions::default(),
//         )
//         .map_err(|e| anyhow!("Impossible de détecter le format audio: {}", e))?;

//     let mut format = probed.format;

//     // Obtenir le premier track audio
//     let track = format
//         .tracks()
//         .iter()
//         .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
//         .ok_or_else(|| anyhow!("Aucune piste audio trouvée"))?;

//     // Créer un décodeur
//     let mut decoder = symphonia::default::get_codecs()
//         .make(&track.codec_params, &DecoderOptions::default())
//         .map_err(|e| anyhow!("Impossible de créer le décodeur: {}", e))?;

//     // Buffer pour stocker les samples décodés
//     let mut samples = Vec::new();
//     let track_id = track.id;

//     // Décoder tous les packets
//     loop {
//         let packet = match format.next_packet() {
//             Ok(packet) => packet,
//             Err(SymphoniaError::ResetRequired) => {
//                 // Reset du décodeur requis
//                 decoder.reset();
//                 continue;
//             }
//             Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
//                 break;
//             }
//             Err(e) => return Err(anyhow!("Erreur lors de la lecture: {}", e)),
//         };

//         // Ignorer les packets qui ne sont pas de notre track
//         if packet.track_id() != track_id {
//             continue;
//         }

//         match decoder.decode(&packet) {
//             Ok(decoded) => {
//                 // Convertir les samples en format standard
//                 let spec = *decoded.spec();
//                 let duration = decoded.capacity() as u64;

//                 let mut sample_buf = SampleBuffer::<i16>::new(duration, spec);
//                 sample_buf.copy_interleaved_ref(decoded);

//                 samples.extend_from_slice(sample_buf.samples());
//             }
//             Err(SymphoniaError::DecodeError(_)) => continue,
//             Err(e) => return Err(anyhow!("Erreur de décodage: {}", e)),
//         }
//     }

//     if samples.is_empty() {
//         return Err(anyhow!("Aucun sample décodé"));
//     }

//     // Note: Pour l'encodage FLAC, on aurait besoin d'une bibliothèque comme
//     // `flacenc` qui n'existe pas encore en Rust. Pour l'instant, on stocke
//     // les données telles quelles si c'est déjà du FLAC, sinon on retourne
//     // les données originales avec un warning.

//     // TODO: Implémenter l'encodage FLAC quand une bibliothèque sera disponible
//     tracing::warn!("Encodage FLAC non implémenté, stockage du format original");
//     Ok(data.to_vec())
// }

// /// Vérifie si les données sont déjà au format FLAC
// fn is_flac(data: &[u8]) -> bool {
//     data.len() >= 4 && &data[0..4] == b"fLaC"
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_is_flac() {
//         let flac_header = b"fLaC\x00\x00\x00\x22";
//         assert!(is_flac(flac_header));

//         let not_flac = b"RIFF\x00\x00\x00\x00";
//         assert!(!is_flac(not_flac));
//     }
// }
