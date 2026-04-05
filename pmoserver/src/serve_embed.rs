//! Remplacement minimal de `axum_embed` compatible avec axum 0.8.
//!
//! Implémente un service Tower qui sert des fichiers embarqués via `rust_embed`,
//! avec support optionnel du mode SPA (fallback vers index.html).

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;
use std::convert::Infallible;
use std::marker::PhantomData;
use std::task::{Context, Poll};
use tower::Service;

/// Service Tower servant des fichiers embarqués via `RustEmbed`.
///
/// - Mode normal (`new`): retourne 404 si le fichier n'existe pas.
/// - Mode SPA (`with_spa_fallback`): retourne le fichier de fallback (200) si le fichier n'existe pas.
#[derive(Clone)]
pub struct ServeEmbed<E> {
    spa_fallback: Option<String>,
    _phantom: PhantomData<E>,
}

impl<E: RustEmbed> ServeEmbed<E> {
    /// Mode normal : 404 pour les fichiers manquants.
    pub fn new() -> Self {
        Self {
            spa_fallback: None,
            _phantom: PhantomData,
        }
    }

    /// Mode SPA : sert `fallback_file` (avec 200) pour les fichiers manquants.
    pub fn with_spa_fallback(fallback_file: String) -> Self {
        Self {
            spa_fallback: Some(fallback_file),
            _phantom: PhantomData,
        }
    }

    fn serve(path: &str) -> Option<Response> {
        let path = path.trim_start_matches('/');
        // Essaie le chemin exact, puis index.html pour les répertoires
        let candidates: &[&str] = if path.is_empty() || path.ends_with('/') {
            &[&format!("{}index.html", path), path]
        } else {
            &[path]
        };

        for candidate in candidates {
            if let Some(content) = E::get(candidate) {
                let mime = mime_guess::from_path(candidate).first_or_octet_stream();
                return Some(
                    (
                        [(header::CONTENT_TYPE, mime.as_ref())],
                        content.data.into_owned(),
                    )
                        .into_response(),
                );
            }
        }
        None
    }
}

impl<E> Service<Request<Body>> for ServeEmbed<E>
where
    E: RustEmbed + Clone + Send + Sync + 'static,
{
    type Response = Response;
    type Error = Infallible;
    type Future = std::future::Ready<Result<Response, Infallible>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let path = req.uri().path();
        let response = Self::serve(path).unwrap_or_else(|| {
            if let Some(ref fallback) = self.spa_fallback {
                Self::serve(fallback).unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        });
        std::future::ready(Ok(response))
    }
}
