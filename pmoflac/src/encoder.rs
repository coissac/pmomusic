use std::{
    ffi::c_void,
    io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, oneshot},
};

use crate::{
    error::FlacError,
    pcm::{PcmChunk, PcmFormat},
    stream::ManagedAsyncReader,
    util::le_bytes_to_interleaved_i32,
};

const CHANNEL_CAPACITY: usize = 8;
const PCM_FRAMES_PER_CHUNK: usize = 4096;

pub struct FlacEncodedStream {
    format: PcmFormat,
    reader: ManagedAsyncReader,
}

impl FlacEncodedStream {
    pub fn format(&self) -> PcmFormat {
        self.format
    }

    pub fn into_parts(self) -> (PcmFormat, ManagedAsyncReader) {
        (self.format, self.reader)
    }

    pub async fn wait(self) -> Result<(), FlacError> {
        self.reader.wait().await
    }
}

impl tokio::io::AsyncRead for FlacEncodedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

#[derive(Debug, Clone)]
pub struct EncoderOptions {
    pub compression_level: u32,
    pub verify: bool,
    pub total_samples: Option<u64>,
    pub block_size: Option<u32>,
}

impl Default for EncoderOptions {
    fn default() -> Self {
        Self {
            compression_level: 5,
            verify: false,
            total_samples: None,
            block_size: None,
        }
    }
}

pub async fn encode_flac_stream<R>(
    reader: R,
    format: PcmFormat,
    options: EncoderOptions,
) -> Result<FlacEncodedStream, FlacError>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    format
        .validate()
        .map_err(|msg| FlacError::Unsupported(format!("invalid PCM format: {msg}")))?;

    if options.compression_level > 12 {
        return Err(FlacError::Unsupported(
            "compression level must be in 0..=12".into(),
        ));
    }

    let (pcm_tx, pcm_rx) = mpsc::channel::<Result<PcmChunk, FlacError>>(CHANNEL_CAPACITY);
    let format_for_reader = format;
    tokio::spawn(async move {
        let _ = feed_pcm_chunks(reader, format_for_reader, pcm_tx).await;
    });

    let (flac_reader, mut flac_writer) = tokio::io::duplex(256 * 1024);
    let (flac_tx, mut flac_rx) = mpsc::channel::<Result<Vec<u8>, FlacError>>(CHANNEL_CAPACITY);
    let (init_tx, init_rx) = oneshot::channel::<Result<(), FlacError>>();

    let format_for_encoder = format;
    let options_for_encoder = options;
    let blocking_handle = tokio::task::spawn_blocking(move || {
        run_encoder(
            format_for_encoder,
            options_for_encoder,
            pcm_rx,
            flac_tx,
            init_tx,
        )
    });

    let writer_handle = tokio::spawn(async move {
        while let Some(chunk) = flac_rx.recv().await {
            let bytes = chunk?;
            if bytes.is_empty() {
                continue;
            }
            flac_writer.write_all(&bytes).await?;
        }
        flac_writer.shutdown().await?;
        match blocking_handle.await {
            Ok(res) => res,
            Err(err) => Err(FlacError::TaskJoin {
                role: "flac-encode",
                details: err.to_string(),
            }),
        }
    });

    init_rx.await.map_err(|_| FlacError::ChannelClosed)??;

    let reader = ManagedAsyncReader::new("flac-encode-writer", flac_reader, writer_handle);
    Ok(FlacEncodedStream { format, reader })
}

async fn feed_pcm_chunks<R>(
    reader: R,
    format: PcmFormat,
    tx: mpsc::Sender<Result<PcmChunk, FlacError>>,
) -> Result<(), FlacError>
where
    R: AsyncRead + Unpin,
{
    let bytes_per_frame = format.bytes_per_sample() * format.channels as usize;
    let chunk_bytes = PCM_FRAMES_PER_CHUNK * bytes_per_frame;
    let mut pending = Vec::with_capacity(chunk_bytes * 2);
    let mut reader = tokio::io::BufReader::new(reader);

    loop {
        while pending.len() >= chunk_bytes {
            let samples =
                le_bytes_to_interleaved_i32(&pending[..chunk_bytes], format.bits_per_sample)
                    .map_err(|msg| FlacError::Encode(msg))?;
            pending.drain(..chunk_bytes);
            let frames = (samples.len() / format.channels as usize) as u32;
            let chunk = PcmChunk::new(samples, frames, format.channels);
            if tx.send(Ok(chunk)).await.is_err() {
                return Ok(());
            }
        }

        let read = reader.read_buf(&mut pending).await?;
        if read == 0 {
            break;
        }
    }

    if !pending.is_empty() {
        if pending.len() % bytes_per_frame != 0 {
            let msg = "PCM stream ended with a partial frame (incomplete sample data)".to_string();
            let _ = tx.send(Err(FlacError::Encode(msg.clone()))).await;
            return Err(FlacError::Encode(msg));
        }
        let samples = le_bytes_to_interleaved_i32(&pending, format.bits_per_sample)
            .map_err(|msg| FlacError::Encode(msg))?;
        let frames = (samples.len() / format.channels as usize) as u32;
        let chunk = PcmChunk::new(samples, frames, format.channels);
        let _ = tx.send(Ok(chunk)).await;
    }

    Ok(())
}

fn run_encoder(
    format: PcmFormat,
    options: EncoderOptions,
    mut rx: mpsc::Receiver<Result<PcmChunk, FlacError>>,
    tx: mpsc::Sender<Result<Vec<u8>, FlacError>>,
    init_tx: oneshot::Sender<Result<(), FlacError>>,
) -> Result<(), FlacError> {
    use libflac_sys::*;

    unsafe {
        let encoder = FLAC__stream_encoder_new();
        if encoder.is_null() {
            let _ = init_tx.send(Err(FlacError::LibFlacInit(
                "FLAC__stream_encoder_new returned null".into(),
            )));
            return Err(FlacError::LibFlacInit(
                "FLAC__stream_encoder_new returned null".into(),
            ));
        }

        let _encoder_guard = EncoderHandle { ptr: encoder };

        let mut state = EncoderClientState::new(tx);

        let ensure = |ok: FLAC__bool, msg: &str| {
            if ok == 0 {
                Err(FlacError::LibFlacInit(msg.into()))
            } else {
                Ok(())
            }
        };

        ensure(
            FLAC__stream_encoder_set_channels(encoder, format.channels as u32),
            "set_channels failed",
        )?;
        ensure(
            FLAC__stream_encoder_set_bits_per_sample(encoder, format.bits_per_sample as u32),
            "set_bits_per_sample failed",
        )?;
        ensure(
            FLAC__stream_encoder_set_sample_rate(encoder, format.sample_rate),
            "set_sample_rate failed",
        )?;
        ensure(
            FLAC__stream_encoder_set_compression_level(encoder, options.compression_level),
            "set_compression_level failed",
        )?;
        ensure(
            FLAC__stream_encoder_set_streamable_subset(encoder, 1),
            "set_streamable_subset failed",
        )?;
        ensure(
            FLAC__stream_encoder_set_verify(encoder, if options.verify { 1 } else { 0 }),
            "set_verify failed",
        )?;
        if let Some(total) = options.total_samples {
            ensure(
                FLAC__stream_encoder_set_total_samples_estimate(encoder, total),
                "set_total_samples_estimate failed",
            )?;
        }
        if let Some(block_size) = options.block_size {
            ensure(
                FLAC__stream_encoder_set_blocksize(encoder, block_size),
                "set_blocksize failed",
            )?;
        }

        let init_status = FLAC__stream_encoder_init_stream(
            encoder,
            Some(write_callback),
            None,
            None,
            None,
            &mut state as *mut EncoderClientState as *mut c_void,
        );
        if init_status != libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_OK {
            let msg = format!("init_stream failed: status {init_status}");
            let _ = init_tx.send(Err(FlacError::LibFlacInit(msg.clone())));
            return Err(FlacError::LibFlacInit(msg));
        }

        let _ = init_tx.send(Ok(()));

        while let Some(chunk_result) = rx.blocking_recv() {
            let chunk = match chunk_result {
                Ok(chunk) => chunk,
                Err(err) => {
                    FLAC__stream_encoder_finish(encoder);
                    return Err(err);
                }
            };
            if chunk.frames == 0 {
                continue;
            }

            let success = FLAC__stream_encoder_process_interleaved(
                encoder,
                chunk.data.as_ptr(),
                chunk.frames,
            );
            if success == 0 {
                if let Some(err) = state.error.take() {
                    FLAC__stream_encoder_finish(encoder);
                    return Err(err);
                }
                FLAC__stream_encoder_finish(encoder);
                return Err(FlacError::Encode("libFLAC reported encode failure".into()));
            }
        }

        let finish_ok = FLAC__stream_encoder_finish(encoder);
        if finish_ok == 0 {
            if let Some(err) = state.error.take() {
                return Err(err);
            }
            return Err(FlacError::Encode(
                "libFLAC failed to finalize stream".into(),
            ));
        }

        if let Some(err) = state.error.take() {
            return Err(err);
        }
    }

    Ok(())
}

struct EncoderClientState {
    tx: mpsc::Sender<Result<Vec<u8>, FlacError>>,
    error: Option<FlacError>,
}

impl EncoderClientState {
    fn new(tx: mpsc::Sender<Result<Vec<u8>, FlacError>>) -> Self {
        Self { tx, error: None }
    }
}

struct EncoderHandle {
    ptr: *mut libflac_sys::FLAC__StreamEncoder,
}

impl Drop for EncoderHandle {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                libflac_sys::FLAC__stream_encoder_delete(self.ptr);
            }
        }
    }
}

unsafe extern "C" fn write_callback(
    _encoder: *const libflac_sys::FLAC__StreamEncoder,
    buffer: *const libflac_sys::FLAC__byte,
    bytes: usize,
    _samples: u32,
    _current_frame: u32,
    client_data: *mut c_void,
) -> libflac_sys::FLAC__StreamEncoderWriteStatus {
    let state = &mut *(client_data as *mut EncoderClientState);
    let slice = std::slice::from_raw_parts(buffer, bytes);
    match state.tx.blocking_send(Ok(slice.to_vec())) {
        Ok(_) => libflac_sys::FLAC__STREAM_ENCODER_WRITE_STATUS_OK,
        Err(_) => {
            state.error = Some(FlacError::LibFlacWrite(
                "failed to send encoded data (receiver dropped)".into(),
            ));
            libflac_sys::FLAC__STREAM_ENCODER_WRITE_STATUS_FATAL_ERROR
        }
    }
}
