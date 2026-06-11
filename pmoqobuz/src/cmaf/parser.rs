use super::error::CmafError;

const QBZ_INIT_UUID: [u8; 16] = [
    0xc7, 0xc7, 0x5d, 0xf0, 0xfd, 0xd9, 0x51, 0xe9,
    0x8f, 0xc2, 0x29, 0x71, 0xe4, 0xac, 0xf8, 0xd2,
];
const QBZ_SEGMENT_UUID: [u8; 16] = [
    0x3b, 0x42, 0x12, 0x92, 0x56, 0xf3, 0x5f, 0x75,
    0x92, 0x36, 0x63, 0xb6, 0x9a, 0x1f, 0x52, 0xb2,
];
const FLAC_MAGIC: &[u8; 4] = b"fLaC";

/// Taille en octets et nombre d'échantillons d'un segment.
#[derive(Debug, Clone)]
pub struct SegmentTableEntry {
    /// Taille des données FLAC déchiffrées de ce segment.
    pub byte_len: u32,
    /// Nombre d'échantillons audio dans ce segment.
    pub sample_count: u32,
}

/// Header FLAC et table des segments extraits du segment d'initialisation.
pub struct InitInfo {
    pub flac_header: Vec<u8>,
    /// Tailles par segment (indices 0..n_segments-1 correspondent aux segments 1..n_segments).
    pub segment_table: Vec<SegmentTableEntry>,
}

/// Une entrée de frame dans le segment UUID box.
pub struct FrameEntry {
    pub size: u32,
    pub flags: u16,
    pub iv: [u8; 8],
}

/// Informations crypto parsées depuis le UUID box d'un segment audio.
pub struct SegmentCrypto {
    /// Offset vers le début des données audio (payload mdat).
    pub data_offset: usize,
    /// Fin du contenu de la mdat box.
    pub mdat_end: usize,
    pub entries: Vec<FrameEntry>,
}

/// Parcourt les boxes ISO BMFF et trouve le premier UUID box correspondant à `target_uuid`.
/// Retourne `(payload_start, box_end)` où payload_start est après les 16 octets UUID.
fn find_uuid_box(data: &[u8], target_uuid: &[u8; 16]) -> Option<(usize, usize)> {
    let mut pos = 0;
    while pos + 8 <= data.len() {
        let size = read_box_size(data, pos);
        if size < 8 || pos + size > data.len() {
            break;
        }
        if &data[pos + 4..pos + 8] == b"uuid" && pos + 24 <= data.len() {
            if &data[pos + 8..pos + 24] == target_uuid.as_ref() {
                return Some((pos + 24, pos + size));
            }
        }
        pos += size;
    }
    None
}

/// Parse le segment d'initialisation (segment 0) pour extraire le header FLAC et la table des segments.
pub fn parse_init_segment(data: &[u8]) -> Result<InitInfo, CmafError> {
    let (payload_start, box_end) = find_uuid_box(data, &QBZ_INIT_UUID)
        .ok_or_else(|| CmafError::ParseError("segment init: QBZ_INIT_UUID box non trouvé".into()))?;

    let payload = &data[payload_start..box_end];
    parse_init_uuid_payload(payload)
}

/// Parse un segment audio pour extraire les informations crypto par frame.
pub fn parse_segment_crypto(data: &[u8]) -> Result<SegmentCrypto, CmafError> {
    let mut uuid_box_start: Option<usize> = None;
    let mut mdat_end = data.len();

    let mut pos = 0;
    while pos + 8 <= data.len() {
        let size = read_box_size(data, pos);
        if size < 8 || pos + size > data.len() {
            break;
        }
        let box_type = &data[pos + 4..pos + 8];
        if box_type == b"uuid" && pos + 24 <= data.len() {
            if &data[pos + 8..pos + 24] == QBZ_SEGMENT_UUID.as_ref() {
                uuid_box_start = Some(pos);
            }
        } else if box_type == b"mdat" {
            mdat_end = pos + size;
        }
        pos += size;
    }

    let box_start = uuid_box_start
        .ok_or_else(|| CmafError::ParseError("segment audio: QBZ_SEGMENT_UUID box non trouvé".into()))?;

    parse_segment_uuid_payload(data, box_start, mdat_end)
}

fn parse_init_uuid_payload(payload: &[u8]) -> Result<InitInfo, CmafError> {
    // Layout payload:
    //   [4B padding/version]
    //   [4B track_id]
    //   [4B file_id]
    //   [4B sample_rate]
    //   [1B bits_per_sample]
    //   [1B channels + 2B padding]
    //   [6B total_samples_count]
    //   [2B raw_data_len]
    //   [raw_data_len bytes: contient le header FLAC]
    //   [1B key_id_len]
    //   [key_id_len bytes: key_id]
    //   [2B segment_count]
    //   Par segment: [4B byte_len][4B sample_count]

    if payload.len() < 28 {
        return Err(CmafError::ParseError("payload init UUID trop court".into()));
    }

    let mut a = 4; // version/padding
    a += 4; // track_id
    a += 4; // file_id
    a += 4; // sample_rate
    a += 1; // bits_per_sample
    a += 3; // channels + padding
    a += 6; // total_samples_count

    if a + 2 > payload.len() {
        return Err(CmafError::ParseError("payload init UUID tronqué au raw_len".into()));
    }
    let raw_len = u16::from_be_bytes([payload[a], payload[a + 1]]) as usize;
    a += 2;

    let raw_data = &payload[a..a + raw_len.min(payload.len() - a)];
    a += raw_len;

    let flac_pos = raw_data
        .windows(4)
        .position(|w| w == FLAC_MAGIC)
        .ok_or_else(|| CmafError::ParseError("payload init UUID: magic fLaC non trouvé".into()))?;

    // fLaC (4) + STREAMINFO block header (4) + STREAMINFO data (34) = 42 octets
    let header_len = 4 + 4 + 34;
    if flac_pos + header_len > raw_data.len() {
        return Err(CmafError::ParseError("payload init UUID: STREAMINFO tronqué".into()));
    }

    let mut flac_header = raw_data[flac_pos..flac_pos + header_len].to_vec();
    // Marquer le dernier bloc de métadonnées
    flac_header[4] |= 0x80;

    if a + 1 > payload.len() {
        return Ok(InitInfo { flac_header, segment_table: Vec::new() });
    }
    let key_id_len = payload[a] as usize;
    a += 1 + key_id_len;

    let mut segment_table = Vec::new();
    if a + 2 <= payload.len() {
        let seg_count = u16::from_be_bytes([payload[a], payload[a + 1]]) as usize;
        a += 2;

        for _ in 0..seg_count {
            if a + 8 > payload.len() {
                break;
            }
            let byte_len = u32::from_be_bytes([payload[a], payload[a + 1], payload[a + 2], payload[a + 3]]);
            a += 4;
            let sample_count = u32::from_be_bytes([payload[a], payload[a + 1], payload[a + 2], payload[a + 3]]);
            a += 4;
            segment_table.push(SegmentTableEntry { byte_len, sample_count });
        }
    }

    tracing::debug!(
        "Init UUID: {} segments dans la table, header FLAC {} octets",
        segment_table.len(),
        flac_header.len()
    );

    Ok(InitInfo { flac_header, segment_table })
}

fn parse_segment_uuid_payload(
    data: &[u8],
    uuid_box_start: usize,
    mdat_end: usize,
) -> Result<SegmentCrypto, CmafError> {
    // Layout après box header (8) + UUID (16) = offset 24 depuis uuid_box_start:
    //   [4B version/padding]
    //   [4B data_offset_raw]   — offset depuis uuid_box_start vers les données audio
    //   [1B iv_size]
    //   [3B frame_count (24-bit BE)]
    //   Par frame: [4B size][2B skip][2B flags][iv_size bytes IV]

    let base = uuid_box_start + 24;
    if base + 12 > data.len() {
        return Err(CmafError::ParseError(
            "payload segment UUID trop court pour le header".into(),
        ));
    }

    let mut a = base + 4; // skip 4-byte version/padding

    let data_offset_raw = u32::from_be_bytes([data[a], data[a + 1], data[a + 2], data[a + 3]]);
    let data_offset = uuid_box_start + data_offset_raw as usize;
    a += 4;

    let iv_size = data[a] as usize;
    a += 1;

    let frame_count =
        ((data[a] as usize) << 16) | ((data[a + 1] as usize) << 8) | (data[a + 2] as usize);
    a += 3;

    let entry_size = 4 + 2 + 2 + iv_size;
    if a + frame_count * entry_size > data.len() {
        return Err(CmafError::ParseError(format!(
            "segment UUID: données insuffisantes pour {frame_count} entrées de {entry_size} octets"
        )));
    }

    let mut entries = Vec::with_capacity(frame_count);
    for _ in 0..frame_count {
        let size = u32::from_be_bytes([data[a], data[a + 1], data[a + 2], data[a + 3]]);
        a += 4;
        a += 2; // 2 octets inconnus
        let flags = u16::from_be_bytes([data[a], data[a + 1]]);
        a += 2;

        let mut iv = [0u8; 8];
        let copy_len = iv_size.min(8);
        iv[..copy_len].copy_from_slice(&data[a..a + copy_len]);
        a += iv_size;

        entries.push(FrameEntry { size, flags, iv });
    }

    Ok(SegmentCrypto { data_offset, mdat_end, entries })
}

fn read_box_size(data: &[u8], pos: usize) -> usize {
    if pos + 8 > data.len() {
        return 0;
    }
    let s = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
    match s {
        0 => data.len() - pos,
        1..=7 => 0,
        s => s as usize,
    }
}
