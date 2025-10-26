use bytes::Bytes;
use futures_util::{stream, Future, Stream, StreamExt};
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::RwLock;
use tokio_util::io::ReaderStream;

/// Type pour une fonction de transformation de stream.
///
/// La fonction reçoit :
/// - Un `CacheInput` abstrait (HTTP ou lecteur en streaming)
/// - Un writer pour écrire les données transformées
/// - Un callback pour mettre à jour la progression
///
/// Elle retourne un `Future` qui se résout en `Result`.
pub type StreamTransformer = Box<
    dyn FnOnce(
            CacheInput,
            tokio::fs::File,
            Arc<dyn Fn(u64) + Send + Sync>,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>
        + Send,
>;

type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, String>> + Send>>;

/// Source générique (HTTP ou lecteur) exposée aux transformers.
pub struct CacheInput {
    inner: CacheInputInner,
}

enum CacheInputInner {
    Http {
        response: Option<reqwest::Response>,
        buffer: Option<Bytes>,
        length: Option<u64>,
    },
    Reader {
        reader: Option<Box<dyn AsyncRead + Send + Unpin>>,
        buffer: Option<Bytes>,
        length: Option<u64>,
    },
}

impl CacheInput {
    pub fn from_response(response: reqwest::Response) -> Self {
        let length = response.content_length();
        Self {
            inner: CacheInputInner::Http {
                response: Some(response),
                buffer: None,
                length,
            },
        }
    }

    pub fn from_reader<R>(reader: R, length: Option<u64>) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
    {
        Self::from_reader_box(Box::new(reader), length)
    }

    pub fn from_reader_box(reader: Box<dyn AsyncRead + Send + Unpin>, length: Option<u64>) -> Self {
        Self {
            inner: CacheInputInner::Reader {
                reader: Some(reader),
                buffer: None,
                length,
            },
        }
    }

    pub fn content_length(&self) -> Option<u64> {
        match &self.inner {
            CacheInputInner::Http { length, buffer, .. } => {
                length.or_else(|| buffer.as_ref().map(|b| b.len() as u64))
            }
            CacheInputInner::Reader { length, buffer, .. } => {
                length.or_else(|| buffer.as_ref().map(|b| b.len() as u64))
            }
        }
    }

    pub async fn bytes(&mut self) -> Result<Bytes, String> {
        match &mut self.inner {
            CacheInputInner::Http {
                response, buffer, ..
            } => {
                if let Some(bytes) = buffer.clone() {
                    return Ok(bytes);
                }

                let resp = response
                    .take()
                    .ok_or_else(|| "stream already consumed".to_string())?;
                let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
                *buffer = Some(bytes.clone());
                Ok(bytes)
            }
            CacheInputInner::Reader { reader, buffer, .. } => {
                if let Some(bytes) = buffer.clone() {
                    return Ok(bytes);
                }

                let mut reader = reader
                    .take()
                    .ok_or_else(|| "stream already consumed".to_string())?;

                let mut data = Vec::new();
                reader
                    .read_to_end(&mut data)
                    .await
                    .map_err(|e| e.to_string())?;

                let bytes = Bytes::from(data);
                *buffer = Some(bytes.clone());
                Ok(bytes)
            }
        }
    }

    pub fn into_byte_stream(self) -> ByteStream {
        match self.inner {
            CacheInputInner::Http {
                response, buffer, ..
            } => {
                if let Some(response) = response {
                    Box::pin(
                        response
                            .bytes_stream()
                            .map(|res| res.map_err(|e| e.to_string())),
                    )
                } else if let Some(bytes) = buffer {
                    Box::pin(stream::once(async move { Ok(bytes) }))
                } else {
                    Box::pin(stream::once(async {
                        Err("stream already consumed".to_string())
                    }))
                }
            }
            CacheInputInner::Reader { reader, buffer, .. } => {
                if let Some(bytes) = buffer {
                    Box::pin(stream::once(async move { Ok(bytes) }))
                } else if let Some(reader) = reader {
                    let stream = ReaderStream::new(reader);
                    Box::pin(stream.map(|res| {
                        res.map(Bytes::from)
                            .map_err(|e| format!("Stream read error: {}", e))
                    }))
                } else {
                    Box::pin(stream::once(async {
                        Err("stream already consumed".to_string())
                    }))
                }
            }
        }
    }
}

enum DownloadSource {
    Url(String),
    Reader {
        reader: Box<dyn AsyncRead + Send + Unpin>,
        length: Option<u64>,
    },
}

/// État interne du téléchargement
#[derive(Debug, Clone)]
struct DownloadState {
    current_size: u64,
    expected_size: Option<u64>,
    transformed_size: u64,
    finished: bool,
    read_position: u64,
    error: Option<String>,
}

/// Objet représentant un téléchargement en cours
#[derive(Debug)]
pub struct Download {
    filename: PathBuf,
    state: Arc<RwLock<DownloadState>>,
}

impl Download {
    fn new(filename: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            filename,
            state: Arc::new(RwLock::new(DownloadState {
                current_size: 0,
                expected_size: None,
                transformed_size: 0,
                finished: false,
                read_position: 0,
                error: None,
            })),
        })
    }

    pub fn filename(&self) -> &Path {
        &self.filename
    }

    pub async fn wait_until_min_size(&self, min_size: u64) -> Result<(), String> {
        loop {
            let state = self.state.read().await;
            if let Some(err) = &state.error {
                return Err(err.clone());
            }
            if state.transformed_size >= min_size || state.finished {
                return Ok(());
            }
            drop(state);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub async fn wait_until_finished(&self) -> Result<(), String> {
        loop {
            let state = self.state.read().await;
            if let Some(err) = &state.error {
                return Err(err.clone());
            }
            if state.finished {
                return Ok(());
            }
            drop(state);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub fn open(&self) -> io::Result<File> {
        File::open(&self.filename)
    }

    pub async fn pos(&self) -> u64 {
        let state = self.state.read().await;
        state.read_position
    }

    pub async fn set_pos(&self, pos: u64) {
        let mut state = self.state.write().await;
        state.read_position = pos;
    }

    pub async fn expected_size(&self) -> Option<u64> {
        let state = self.state.read().await;
        state.expected_size
    }

    pub async fn current_size(&self) -> u64 {
        let state = self.state.read().await;
        state.current_size
    }

    pub async fn transformed_size(&self) -> u64 {
        let state = self.state.read().await;
        state.transformed_size
    }

    pub async fn finished(&self) -> bool {
        let state = self.state.read().await;
        state.finished
    }

    pub async fn error(&self) -> Option<String> {
        let state = self.state.read().await;
        state.error.clone()
    }
}

/// Lance le téléchargement d'une URL dans un fichier.
pub fn download<P: AsRef<Path>>(filename: P, url: &str) -> Arc<Download> {
    download_with_transformer(filename, url, None)
}

/// Lance le téléchargement d'une URL avec transformation du stream.
pub fn download_with_transformer<P: AsRef<Path>>(
    filename: P,
    url: &str,
    transformer: Option<StreamTransformer>,
) -> Arc<Download> {
    spawn_download(filename, DownloadSource::Url(url.to_string()), transformer)
}

/// Ingère un flux (AsyncRead) dans le cache avec transformation optionnelle.
pub fn ingest_with_transformer<P, R>(
    filename: P,
    reader: R,
    length: Option<u64>,
    transformer: Option<StreamTransformer>,
) -> Arc<Download>
where
    P: AsRef<Path>,
    R: AsyncRead + Send + Unpin + 'static,
{
    spawn_download(
        filename,
        DownloadSource::Reader {
            reader: Box::new(reader),
            length,
        },
        transformer,
    )
}

fn spawn_download<P: AsRef<Path>>(
    filename: P,
    source: DownloadSource,
    transformer: Option<StreamTransformer>,
) -> Arc<Download> {
    let filename = filename.as_ref().to_path_buf();
    let download = Download::new(filename.clone());
    let state = Arc::clone(&download.state);

    tokio::spawn(async move {
        if let Err(e) = download_impl(filename, source, state, transformer).await {
            tracing::error!("Download error: {}", e);
        }
    });

    download
}

async fn download_impl(
    filename: PathBuf,
    source: DownloadSource,
    state: Arc<RwLock<DownloadState>>,
    transformer: Option<StreamTransformer>,
) -> Result<(), String> {
    let input = match source {
        DownloadSource::Url(url) => {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .map_err(|e| e.to_string())?;

            let response = match client.get(&url).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    let mut s = state.write().await;
                    let error = format!("Failed to fetch URL '{}': {}", url, e);
                    s.error = Some(error.clone());
                    s.finished = true;
                    return Err(error);
                }
            };

            if !response.status().is_success() {
                let mut s = state.write().await;
                let error = format!("HTTP error: {}", response.status());
                s.error = Some(error.clone());
                s.finished = true;
                return Err(error);
            }

            let length = response.content_length();
            {
                let mut s = state.write().await;
                s.expected_size = length;
            }

            CacheInput::from_response(response)
        }
        DownloadSource::Reader { reader, length } => {
            {
                let mut s = state.write().await;
                s.expected_size = length;
            }
            CacheInput::from_reader_box(reader, length)
        }
    };

    let file = tokio::fs::File::create(&filename)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    process_input(input, file, state, transformer).await
}

async fn process_input(
    input: CacheInput,
    file: tokio::fs::File,
    state: Arc<RwLock<DownloadState>>,
    transformer: Option<StreamTransformer>,
) -> Result<(), String> {
    if let Some(transformer) = transformer {
        let progress_state = Arc::clone(&state);
        let progress_callback: Arc<dyn Fn(u64) + Send + Sync> =
            Arc::new(move |transformed_bytes| {
                let progress_state = Arc::clone(&progress_state);
                tokio::spawn(async move {
                    let mut s = progress_state.write().await;
                    s.transformed_size = transformed_bytes;
                });
            });

        match transformer(input, file, Arc::clone(&progress_callback)).await {
            Ok(_) => {
                let mut s = state.write().await;
                if s.current_size == 0 {
                    s.current_size = s.transformed_size;
                }
                s.finished = true;
                Ok(())
            }
            Err(e) => {
                let mut s = state.write().await;
                s.error = Some(e.clone());
                s.finished = true;
                Err(e)
            }
        }
    } else {
        default_copy(input, file, state).await
    }
}

async fn default_copy(
    input: CacheInput,
    mut file: tokio::fs::File,
    state: Arc<RwLock<DownloadState>>,
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;

    let mut stream = input.into_byte_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        if let Err(e) = file.write_all(&chunk).await {
            let mut s = state.write().await;
            let error = format!("Failed to write to file: {}", e);
            s.error = Some(error.clone());
            s.finished = true;
            return Err(error);
        }

        let mut s = state.write().await;
        let len = chunk.len() as u64;
        s.current_size += len;
        s.transformed_size += len;
    }

    if let Err(e) = file.flush().await {
        let mut s = state.write().await;
        let error = format!("Failed to flush file: {}", e);
        s.error = Some(error.clone());
        s.finished = true;
        return Err(error);
    }

    let mut s = state.write().await;
    s.finished = true;
    Ok(())
}

/// Lit les premiers octets d'une URL sans télécharger le fichier complet
///
/// Cette fonction effectue une requête HTTP partielle (Range header) pour télécharger
/// uniquement les premiers octets d'un fichier. C'est utilisé pour calculer l'identifiant
/// basé sur le contenu sans avoir à télécharger tout le fichier.
///
/// # Arguments
///
/// * `url` - URL du fichier à télécharger
/// * `max_bytes` - Nombre maximum d'octets à lire (par défaut 512)
///
/// # Returns
///
/// Un `Vec<u8>` contenant les premiers octets du fichier
///
/// # Exemple
///
/// ```rust,ignore
/// use pmocache::download::peek_header;
///
/// let header = peek_header("http://example.com/file.dat", 512).await?;
/// let pk = pk_from_content_header(&header);
/// ```
pub async fn peek_header(url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    // Essayer d'abord avec une requête Range
    let range_header = format!("bytes=0-{}", max_bytes - 1);
    let mut response = client
        .get(url)
        .header("Range", range_header)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch URL '{}': {}", url, e))?;

    // Si le serveur ne supporte pas Range (status 200 au lieu de 206),
    // on lit quand même mais on limite la lecture
    if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
    {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let mut buffer = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        buffer.extend_from_slice(&chunk);
        if buffer.len() >= max_bytes {
            buffer.truncate(max_bytes);
            break;
        }
    }

    Ok(buffer)
}

/// Lit les premiers octets d'un reader asynchrone
///
/// Cette fonction lit jusqu'à `max_bytes` octets depuis un reader asynchrone.
/// C'est utilisé pour calculer l'identifiant basé sur le contenu des fichiers locaux
/// ou des streams.
///
/// # Arguments
///
/// * `reader` - Le reader asynchrone à lire
/// * `max_bytes` - Nombre maximum d'octets à lire (par défaut 512)
///
/// # Returns
///
/// Un `Vec<u8>` contenant les premiers octets lus
///
/// # Exemple
///
/// ```rust,ignore
/// use pmocache::download::peek_reader_header;
/// use tokio::fs::File;
///
/// let mut file = File::open("file.dat").await?;
/// let header = peek_reader_header(&mut file, 512).await?;
/// let pk = pk_from_content_header(&header);
/// ```
pub async fn peek_reader_header<R>(reader: &mut R, max_bytes: usize) -> Result<Vec<u8>, String>
where
    R: AsyncRead + Unpin,
{
    let mut buffer = vec![0u8; max_bytes];
    let n = reader
        .read(&mut buffer)
        .await
        .map_err(|e| format!("Failed to read from stream: {}", e))?;
    buffer.truncate(n);
    Ok(buffer)
}
