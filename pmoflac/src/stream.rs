use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::{
    io::{AsyncRead, DuplexStream, ReadBuf},
    task::JoinHandle,
};

use crate::error::FlacError;

/// Async reader that is backed by a spawned task writing into it.
pub struct ManagedAsyncReader {
    inner: Option<DuplexStream>,
    join: Option<JoinHandle<Result<(), FlacError>>>,
    role: &'static str,
}

impl ManagedAsyncReader {
    pub fn new(
        role: &'static str,
        inner: DuplexStream,
        join: JoinHandle<Result<(), FlacError>>,
    ) -> Self {
        Self {
            inner: Some(inner),
            join: Some(join),
            role,
        }
    }

    /// Waits for the producer task to finish.
    pub async fn wait(mut self) -> Result<(), FlacError> {
        match self.join.take() {
            Some(handle) => match handle.await {
                Ok(res) => res,
                Err(err) => Err(FlacError::TaskJoin {
                    role: self.role,
                    details: err.to_string(),
                }),
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

impl AsyncRead for ManagedAsyncReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.poll_read_inner(cx, buf)
    }
}

impl Drop for ManagedAsyncReader {
    fn drop(&mut self) {
        if let Some(handle) = self.join.take() {
            handle.abort();
        }
        self.inner.take();
    }
}
