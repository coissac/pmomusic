//! Implémentation du trait UpnpServer pour le serveur pmoserver
//!
//! Ce module fournit l'implémentation du trait [`pmoupnp::UpnpServer`] pour
//! le [`Server`](crate::server::Server) de pmoserver, permettant aux devices
//! et services UPnP d'enregistrer automatiquement leurs endpoints HTTP.
//!
//! ## Architecture
//!
//! L'implémentation fait le pont entre :
//! - Les pointeurs de fonction du trait `UpnpServer` (agnostiques du framework web)
//! - Les handlers Axum (spécifiques à l'implémentation `pmoserver`)
//!
//! Chaque méthode du trait crée un wrapper qui :
//! 1. Convertit les pointeurs de fonction en closures compatibles Axum
//! 2. Délègue l'enregistrement aux méthodes internes du `Server`
//! 3. Retourne une future qui se résout une fois le handler enregistré
//!
//! ## Exemple d'utilisation
//!
//! ```rust,no_run
//! use pmoupnp::{UpnpServer, mediarenderer::device::MEDIA_RENDERER};
//! use pmoupnp::devices::DeviceInstance;
//! use pmoserver::ServerBuilder;
//! use std::sync::Arc;
//!
//! # async fn example() {
//! let mut server = ServerBuilder::new("MyRenderer").build();
//! let device = Arc::new(DeviceInstance::new(&MEDIA_RENDERER));
//!
//! // Le trait UpnpServer est automatiquement disponible
//! device.register_urls(&mut server).await;
//! # }
//! ```

use crate::server::Server;
use pmoupnp::{UpnpServer, server::{Response, HeaderMap, Request}};
use std::future::Future;
use std::pin::Pin;
use axum::extract::State;

impl UpnpServer for Server {
    fn add_handler<F, Fut>(&mut self, path: &str, handler: F) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
    where
        F: Fn() -> Fut + Send + Sync + 'static + Clone,
        Fut: Future<Output = Response> + Send + 'static,
    {
        let path = path.to_string();
        Box::pin(async move {
            Self::add_handler(self, &path, handler).await;
        })
    }

    fn add_post_handler_with_state<S>(
        &mut self,
        path: &str,
        handler: fn(State<S>, String) -> Pin<Box<dyn Future<Output = Response> + Send>>,
        state: S,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
    where
        S: Clone + Send + Sync + 'static,
    {
        let path = path.to_string();

        // Créer un wrapper qui convertit le fn pointer en handler Axum
        let wrapper = move |State(s): State<S>, body: String| -> Pin<Box<dyn Future<Output = Response> + Send>> {
            handler(State(s), body)
        };

        Box::pin(async move {
            Self::add_post_handler_with_state(self, &path, wrapper, state).await;
        })
    }

    fn add_handler_with_state<S>(
        &mut self,
        path: &str,
        handler: fn(State<S>, HeaderMap, Request) -> Pin<Box<dyn Future<Output = Response> + Send>>,
        state: S,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
    where
        S: Clone + Send + Sync + 'static,
    {
        let path = path.to_string();

        // Créer un wrapper qui convertit le fn pointer en handler Axum
        let wrapper = move |State(s): State<S>, headers: HeaderMap, req: Request| -> Pin<Box<dyn Future<Output = Response> + Send>> {
            handler(State(s), headers, req)
        };

        Box::pin(async move {
            Self::add_handler_with_state(self, &path, wrapper, state).await;
        })
    }
}
