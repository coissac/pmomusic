//! Node de conversion qui s'assure que chaque `TrackBoundary` possède un `cover_pk`.
//!
//! Il laisse passer tous les segments audio de manière transparente. Lorsqu'un
//! `TrackBoundary` est détecté, il vérifie si ses métadonnées contiennent déjà
//! un `cover_pk`. Si ce n'est pas le cas mais qu'une `cover_url` est disponible,
//! l'image est sauvegardée dans le cache de couvertures puis la clé primaire est
//! écrite dans les métadonnées avant de poursuivre la propagation.

use std::sync::Arc;

use pmoaudio::{
    nodes::{AudioError, DEFAULT_CHANNEL_SIZE},
    pipeline::{send_to_children, AudioPipelineNode, Node, NodeLogic, PipelineHandle},
    AudioSegment, TypeRequirement, TypedAudioNode,
};
use pmocovers::Cache as CoverCache;
use pmometadata::TrackMetadata;
use tokio::select;
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// Node enveloppe qui applique [`TrackBoundaryCoverLogic`].
pub struct TrackBoundaryCoverNode {
    inner: Node<TrackBoundaryCoverLogic>,
}

impl TrackBoundaryCoverNode {
    /// Crée un nouveau node.
    pub fn new(cover_cache: Arc<CoverCache>) -> Self {
        let logic = TrackBoundaryCoverLogic::new(cover_cache);
        Self {
            inner: Node::new_with_input(logic, DEFAULT_CHANNEL_SIZE),
        }
    }
}

#[async_trait::async_trait]
impl AudioPipelineNode for TrackBoundaryCoverNode {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
        self.inner.get_tx()
    }

    fn register(&mut self, child: Box<dyn AudioPipelineNode>) {
        self.inner.register(child);
    }

    async fn run(self: Box<Self>, stop_token: CancellationToken) -> Result<(), AudioError> {
        Box::new(self.inner).run(stop_token).await
    }

    fn start(self: Box<Self>) -> PipelineHandle {
        Box::new(self.inner).start()
    }
}

impl TypedAudioNode for TrackBoundaryCoverNode {
    fn input_type(&self) -> Option<TypeRequirement> {
        Some(TypeRequirement::any())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        Some(TypeRequirement::any())
    }
}

struct TrackBoundaryCoverLogic {
    cover_cache: Arc<CoverCache>,
}

impl TrackBoundaryCoverLogic {
    fn new(cover_cache: Arc<CoverCache>) -> Self {
        Self { cover_cache }
    }

    async fn ensure_cover_pk(&self, metadata: Arc<RwLock<dyn TrackMetadata>>) {
        let cover_url = {
            let guard = metadata.read().await;

            match guard.get_cover_pk().await {
                Ok(Some(pk)) => {
                    debug!("TrackBoundaryCoverNode: cover_pk already set ({})", pk);
                    return;
                }
                Ok(None) => {}
                Err(err) => warn!("TrackBoundaryCoverNode: cannot read cover_pk: {}", err),
            }

            match guard.get_cover_url().await {
                Ok(url) => url,
                Err(err) => {
                    warn!("TrackBoundaryCoverNode: cannot read cover_url: {}", err);
                    return;
                }
            }
        };

        let cover_url = match cover_url {
            Some(url) => url,
            None => {
                debug!("TrackBoundaryCoverNode: no cover_url present, skipping cache");
                return;
            }
        };

        match self.cover_cache.add_from_url(&cover_url, None).await {
            Ok(pk) => {
                debug!(
                    "TrackBoundaryCoverNode: cached cover for url={}, pk={}",
                    cover_url, pk
                );
                let mut guard = metadata.write().await;
                if let Err(err) = guard.set_cover_pk(Some(pk.clone())).await {
                    warn!(
                        "TrackBoundaryCoverNode: failed to set cover_pk {}: {}",
                        pk, err
                    );
                }
            }
            Err(err) => {
                warn!(
                    "TrackBoundaryCoverNode: failed to cache cover from {}: {}",
                    cover_url, err
                );
            }
        }
    }
}

#[async_trait::async_trait]
impl NodeLogic for TrackBoundaryCoverLogic {
    async fn process(
        &mut self,
        input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        let mut input = input.ok_or_else(|| {
            AudioError::ProcessingError(
                "TrackBoundaryCoverNode requires an upstream input channel".into(),
            )
        })?;
        let node_name = std::any::type_name::<Self>();

        loop {
            let segment = select! {
                _ = stop_token.cancelled() => {
                    debug!("TrackBoundaryCoverNode: stop requested");
                    break;
                }
                segment = input.recv() => segment,
            };

            let Some(segment) = segment else {
                debug!("TrackBoundaryCoverNode: upstream closed");
                break;
            };

            if let Some(metadata) = segment.as_track_metadata() {
                self.ensure_cover_pk(Arc::clone(metadata)).await;
            }

            send_to_children(node_name, &output, segment).await?;
        }

        Ok(())
    }
}
