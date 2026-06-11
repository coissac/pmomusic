//! Helper de retry avec backoff exponentiel pour les fetches réseau transitoires.
//!
//! Un blip réseau transitoire (5xx, timeout, connexion reset, 429) sur le
//! `file/url` du prochain track ou un segment CMAF ne doit pas être fatal.
//! Ce module retente les échecs *transitoires* avec backoff exponentiel
//! et laisse les échecs *terminaux* (404 "disparu définitivement", erreurs auth)
//! se propager immédiatement.

use std::future::Future;
use std::time::Duration;

/// Nombre de tentatives : 1 initiale + 2 retrys.
pub const DEFAULT_MAX_ATTEMPTS: u32 = 3;

/// Erreur de fetch taguée selon son caractère retryable.
#[derive(Debug)]
pub enum FetchError {
    /// Vaut la peine de retenter : erreur réseau/timeout/connect/body, 5xx, ou 429.
    Transient(String),
    /// Ne vaut pas la peine de retenter : 4xx (sauf 429), ou échec définitif.
    Terminal(String),
}

impl FetchError {
    pub fn is_transient(&self) -> bool {
        matches!(self, FetchError::Transient(_))
    }
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Transient(s) | FetchError::Terminal(s) => write!(f, "{}", s),
        }
    }
}

/// Vrai pour les erreurs reqwest qui valent la peine d'être retentées.
pub fn reqwest_is_transient(e: &reqwest::Error) -> bool {
    e.is_timeout() || e.is_connect() || e.is_request() || e.is_body()
}

/// Classifie une erreur reqwest en `FetchError`.
/// Toutes les erreurs transport reqwest sont traitées comme transitoires.
pub fn classify_reqwest(e: &reqwest::Error, context: &str) -> FetchError {
    FetchError::Transient(format!("{}: {}", context, e))
}

/// Classifie un status HTTP non-succès en `FetchError`.
/// 5xx et 429 → transitoire ; tout le reste (404, 403, ...) → terminal.
pub fn classify_status(status: reqwest::StatusCode, context: &str) -> FetchError {
    let msg = format!("{}: HTTP {}", context, status);
    if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        FetchError::Transient(msg)
    } else {
        FetchError::Terminal(msg)
    }
}

/// Backoff exponentiel avec jitter pour la N-ième tentative (base 1) :
/// ~250 ms, ~500 ms, ~1 s, plafonné à 2 s, plus jusqu'à +25% de jitter.
/// Le jitter est dérivé de l'horloge pour éviter une dépendance à `rand`.
fn backoff_delay(attempt: u32) -> Duration {
    let exp = attempt.saturating_sub(1).min(3);
    let base_ms = 250u64.saturating_mul(1u64 << exp).min(2000);
    let jitter_span = base_ms / 4;
    let jitter = if jitter_span > 0 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0);
        nanos % (jitter_span + 1)
    } else {
        0
    };
    Duration::from_millis(base_ms + jitter)
}

/// Exécute `op` (qui prend le numéro de tentative base 1) et retente
/// tant qu'il retourne une erreur transitoire, avec backoff entre les tentatives.
/// Les erreurs terminales et la dernière tentative retournent immédiatement.
pub async fn retry_transient<F, Fut, T, E>(
    max_attempts: u32,
    log_tag: &str,
    is_transient: impl Fn(&E) -> bool,
    mut op: F,
) -> std::result::Result<T, E>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = std::result::Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 1;
    loop {
        match op(attempt).await {
            Ok(value) => return Ok(value),
            Err(err) => {
                if attempt >= max_attempts || !is_transient(&err) {
                    return Err(err);
                }
                let delay = backoff_delay(attempt);
                tracing::warn!(
                    "[{}] erreur transitoire tentative {}/{}: {} — retry dans {}ms",
                    log_tag,
                    attempt,
                    max_attempts,
                    err,
                    delay.as_millis()
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn reussit_au_premier_essai() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let r: std::result::Result<u32, FetchError> =
            retry_transient(3, "test", FetchError::is_transient, |_| {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::Relaxed);
                    Ok(42)
                }
            })
            .await;
        assert_eq!(r.unwrap(), 42);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn retente_transitoire_puis_reussit() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let r: std::result::Result<u32, FetchError> =
            retry_transient(3, "test", FetchError::is_transient, |attempt| {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::Relaxed);
                    if attempt < 3 {
                        Err(FetchError::Transient("503".into()))
                    } else {
                        Ok(7)
                    }
                }
            })
            .await;
        assert_eq!(r.unwrap(), 7);
        assert_eq!(calls.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn terminal_ne_retente_pas() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let r: std::result::Result<u32, FetchError> =
            retry_transient(3, "test", FetchError::is_transient, |_| {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::Relaxed);
                    Err(FetchError::Terminal("404".into()))
                }
            })
            .await;
        assert!(r.is_err());
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn abandonne_apres_max_tentatives() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let r: std::result::Result<u32, FetchError> =
            retry_transient(3, "test", FetchError::is_transient, |_| {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::Relaxed);
                    Err(FetchError::Transient("timeout".into()))
                }
            })
            .await;
        assert!(r.is_err());
        assert_eq!(calls.load(Ordering::Relaxed), 3);
    }
}
