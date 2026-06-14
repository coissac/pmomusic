pub mod handler;
pub mod handlers;
pub mod source;

pub use handler::{ResolvedContent, ResolvedTrack, UrlHandler, UrlResolver, UrlResolverError};
pub use handlers::generic::GenericUrlHandler;
pub use handlers::qobuz::QobuzUrlHandler;
pub use handlers::radiofrance::RadioFranceUrlHandler;
pub use source::UrlSource;
