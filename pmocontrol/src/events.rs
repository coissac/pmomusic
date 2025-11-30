use std::sync::{Arc, Mutex};

use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::model::RendererEvent;

#[derive(Clone, Default)]
pub(crate) struct RendererEventBus {
    subscribers: Arc<Mutex<Vec<Sender<RendererEvent>>>>,
}

impl RendererEventBus {
    pub(crate) fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(crate) fn subscribe(&self) -> Receiver<RendererEvent> {
        let (tx, rx) = unbounded::<RendererEvent>();
        {
            let mut subscribers = self.subscribers.lock().unwrap();
            subscribers.push(tx);
        }
        rx
    }

    #[allow(dead_code)]
    pub(crate) fn broadcast(&self, event: RendererEvent) {
        let mut subscribers = self.subscribers.lock().unwrap();
        subscribers.retain(|tx| tx.send(event.clone()).is_ok());
    }
}
