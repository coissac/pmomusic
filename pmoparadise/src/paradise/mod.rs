//! Internal orchestration layer for dynamic Radio Paradise streaming.
//!
//! This module implements the high level structures described in the
//! Radio Paradise functional specification:
//!  - `ParadiseChannel`: lifecycle and state machine for a single RP channel.
//!  - `ParadiseWorker`: async task responsible for polling/downloading blocks.
//!  - `ParadiseClientStream`: per-client audio stream with independent cursor.
//!  - Shared caches and history storage hooked into existing PMO components.
//!
//! The implementation is split across several submodules to keep concerns
//! isolated (constants, playlist management, history persistence, etc.).
//! The goal of this scaffolding is to provide a clear, testable surface for
//! the eventual end-to-end integration with the UPnP server and HTTP routes.

mod channel;
pub mod constants;
mod history;
mod playlist;
mod worker;

pub use channel::{
    max_channel_id, ChannelDescriptor, ParadiseChannel, ParadiseChannelKind, ParadiseClientStream,
    ALL_CHANNELS,
};
pub use constants::*; // Export all constants
pub use history::{create_history_backend, HistoryBackend, HistoryEntry};
pub use playlist::PlaylistEntry;
pub use worker::{load_rp_metadata, ParadiseWorker, RadioParadiseMetadata, WorkerCommand};
