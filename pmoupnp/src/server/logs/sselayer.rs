use tracing::{Event, Subscriber};
use tracing_subscriber::{layer::Context, Layer};
use tracing::field::{Visit, Field};

use super::{LogEntry, LogState};
use std::time::SystemTime;

struct LogVisitor {
    message: String,
}

impl LogVisitor {
    fn new() -> Self {
        Self {
            message: String::new(),
        }
    }
}

impl Visit for LogVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        // capture le champ "message" ou concatène les autres
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        } else {
            if !self.message.is_empty() {
                self.message.push(' ');
            }
            self.message.push_str(&format!("{}={:?}", field.name(), value));
        }
    }
}

/// Layer de tracing qui pousse les events dans le buffer
pub struct SseLayer {
    state: LogState,
}

impl SseLayer {
    pub fn new(state: LogState) -> Self {
        Self { state }
    }
}

impl<S> Layer<S> for SseLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = LogVisitor::new();
        event.record(&mut visitor);

        let entry = LogEntry {
            timestamp: SystemTime::now(),
            level: event.metadata().level().to_string(),
            target: event.metadata().target().to_string(),
            message: visitor.message,
        };

        self.state.push(entry);
    }
}
