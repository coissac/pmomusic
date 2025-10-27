use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::{
    io::{AsyncRead, DuplexStream, ReadBuf},
    task::JoinHandle,
};

/// Async reader that is backed by a spawned task writing into it.
///
/// This is generic over the error type to support both FLAC and MP3 decoders.
pub struct ManagedAsyncReader<E>
where
    E: std::error::Error,
{
    inner: Option<DuplexStream>,
    join: Option<JoinHandle<Result<(), E>>>,
    role: &'static str,
}

impl<E> ManagedAsyncReader<E>
where
    E: std::error::Error,
{
    pub fn new(
        role: &'static str,
        inner: DuplexStream,
        join: JoinHandle<Result<(), E>>,
    ) -> Self {
        Self {
            inner: Some(inner),
            join: Some(join),
            role,
        }
    }

    /// Waits for the producer task to finish.
    pub async fn wait(mut self) -> Result<(), E>
    where
        E: From<String>,
    {
        match self.join.take() {
            Some(handle) => match handle.await {
                Ok(res) => res,
                Err(err) => {
                    let msg = format!("task '{}' failed: {}", self.role, err);
                    Err(E::from(msg))
                }
            },
            None => Ok(()),
        }
    }

    fn poll_read_inner(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let inner = self
            .inner
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "reader dropped"))?;
        Pin::new(inner).poll_read(cx, buf)
    }
}

impl<E> AsyncRead for ManagedAsyncReader<E>
where
    E: std::error::Error,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.poll_read_inner(cx, buf)
    }
}

impl<E> Drop for ManagedAsyncReader<E>
where
    E: std::error::Error,
{
    fn drop(&mut self) {
        if let Some(handle) = self.join.take() {
            handle.abort();
        }
        self.inner.take();
    }
}
