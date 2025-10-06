//! Trait pour les serveurs UPnP
//!
//! Ce module définit le trait [`UpnpServer`] qui permet de connecter
//! des devices UPnP à n'importe quelle implémentation de serveur web.
//!
//! ## Architecture
//!
//! Le trait `UpnpServer` définit une interface minimale permettant aux devices
//! et services UPnP d'enregistrer leurs endpoints HTTP sans dépendre d'une
//! implémentation de serveur spécifique.
//!
//! ## Séparation des responsabilités
//!
//! - **pmoupnp** : Définit le trait `UpnpServer` et l'utilise via des contraintes génériques
//! - **pmoserver** : Fournit une implémentation concrète basée sur Axum
//! - **Autres crates** : Peuvent fournir leurs propres implémentations (actix-web, warp, etc.)
//!
//! ## Exemple d'utilisation
//!
//! ```rust,no_run
//! use pmoupnp::{UpnpServer, devices::{Device, DeviceInstance}};
//! use std::sync::Arc;
//!
//! # async fn example<S: UpnpServer>(mut server: S) {
//! // Créer un device
//! let device = Device::new(
//!     "MyDevice".to_string(),
//!     "MyDeviceType".to_string(),
//!     "Friendly Name".to_string(),
//! );
//! let device_instance = Arc::new(DeviceInstance::new(&device));
//!
//! // Le device enregistre automatiquement ses routes UPnP
//! device_instance.register_urls(&mut server).await;
//! # }
//! ```
//!
//! ## Implémentation
//!
//! Pour implémenter ce trait, votre serveur doit fournir trois méthodes
//! pour enregistrer des handlers HTTP asynchrones :
//!
//! ```rust,no_run
//! use pmoupnp::UpnpServer;
//! use std::future::Future;
//! use std::pin::Pin;
//!
//! struct MyServer {
//!     // votre implémentation
//! }
//!
//! impl UpnpServer for MyServer {
//!     fn add_handler<F, Fut>(&mut self, path: &str, handler: F)
//!         -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
//!     where
//!         F: Fn() -> Fut + Send + Sync + 'static + Clone,
//!         Fut: Future<Output = pmoupnp::server::Response> + Send + 'static,
//!     {
//!         // Enregistrer le handler pour GET requests
//!         # todo!()
//!     }
//!
//!     fn add_post_handler_with_state<S>(
//!         &mut self,
//!         path: &str,
//!         handler: fn(axum::extract::State<S>, String)
//!             -> Pin<Box<dyn Future<Output = pmoupnp::server::Response> + Send>>,
//!         state: S,
//!     ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
//!     where
//!         S: Clone + Send + Sync + 'static,
//!     {
//!         // Enregistrer le handler pour POST avec body
//!         # todo!()
//!     }
//!
//!     fn add_handler_with_state<S>(
//!         &mut self,
//!         path: &str,
//!         handler: fn(axum::extract::State<S>,
//!                    pmoupnp::server::HeaderMap,
//!                    pmoupnp::server::Request)
//!             -> Pin<Box<dyn Future<Output = pmoupnp::server::Response> + Send>>,
//!         state: S,
//!     ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
//!     where
//!         S: Clone + Send + Sync + 'static,
//!     {
//!         // Enregistrer le handler avec accès complet à la requête
//!         # todo!()
//!     }
//! }
//! ```

use std::future::Future;
use std::pin::Pin;

/// Type alias pour la réponse HTTP (basé sur Axum).
///
/// Utilisé pour éviter une dépendance directe sur axum dans les signatures de trait,
/// tout en restant compatible avec les types Axum.
pub type Response = axum::response::Response;

/// Type alias pour les en-têtes HTTP (basé sur Axum).
pub type HeaderMap = axum::http::HeaderMap;

/// Type alias pour la requête HTTP (basé sur Axum).
pub type Request = axum::extract::Request<axum::body::Body>;

/// Trait pour les serveurs compatibles UPnP.
///
/// Ce trait définit l'interface minimale qu'un serveur web doit implémenter
/// pour supporter l'enregistrement automatique des endpoints UPnP par les
/// [`DeviceInstance`](crate::devices::DeviceInstance) et
/// [`ServiceInstance`](crate::services::ServiceInstance).
///
/// ## Contraintes
///
/// - `Send + Sync` : Le serveur doit être partageable entre threads
///
/// ## Méthodes
///
/// Les trois méthodes permettent d'enregistrer différents types de handlers :
///
/// 1. **`add_handler`** : Handler GET simple sans état
/// 2. **`add_post_handler_with_state`** : Handler POST avec état et body texte (pour SOAP)
/// 3. **`add_handler_with_state`** : Handler générique avec accès complet (pour SUBSCRIBE/UNSUBSCRIBE)
///
/// ## Implémentations
///
/// - **pmoserver::Server** : Implémentation basée sur Axum (fournie par la crate `pmoserver`)
pub trait UpnpServer: Send + Sync {
    /// Ajoute un handler GET pour un chemin donné.
    ///
    /// Utilisé principalement pour servir les descripteurs XML des devices et services.
    ///
    /// # Arguments
    ///
    /// * `path` - Le chemin HTTP (ex: `/device/MediaRenderer/description.xml`)
    /// * `handler` - Une closure asynchrone qui génère la réponse
    ///
    /// # Retour
    ///
    /// Une future qui se résout quand le handler est enregistré.
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmoupnp::UpnpServer;
    /// use axum::response::IntoResponse;
    ///
    /// # async fn example<S: UpnpServer>(mut server: S) {
    /// server.add_handler("/description.xml", || async {
    ///     "<?xml version=\"1.0\"?><root></root>".into_response()
    /// }).await;
    /// # }
    /// ```
    fn add_handler<F, Fut>(&mut self, path: &str, handler: F) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
    where
        F: Fn() -> Fut + Send + Sync + 'static + Clone,
        Fut: Future<Output = Response> + Send + 'static;

    /// Ajoute un handler POST avec état pour un chemin donné.
    ///
    /// Utilisé pour les endpoints de contrôle SOAP des services UPnP.
    ///
    /// # Arguments
    ///
    /// * `path` - Le chemin HTTP (ex: `/service/AVTransport/control`)
    /// * `handler` - Un pointeur de fonction qui traite la requête SOAP
    /// * `state` - L'état partagé (typiquement une `ServiceInstance`)
    ///
    /// # Retour
    ///
    /// Une future qui se résout quand le handler est enregistré.
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmoupnp::{UpnpServer, server::Response};
    /// use axum::extract::State;
    /// use std::pin::Pin;
    /// use std::future::Future;
    ///
    /// fn soap_handler(
    ///     State(service): State<String>,
    ///     body: String,
    /// ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
    ///     Box::pin(async move {
    ///         // Traiter la requête SOAP
    ///         axum::response::Response::default()
    ///     })
    /// }
    ///
    /// # async fn example<S: UpnpServer>(mut server: S) {
    /// server.add_post_handler_with_state(
    ///     "/control",
    ///     soap_handler,
    ///     "ServiceName".to_string(),
    /// ).await;
    /// # }
    /// ```
    fn add_post_handler_with_state<S>(
        &mut self,
        path: &str,
        handler: fn(axum::extract::State<S>, String) -> Pin<Box<dyn Future<Output = Response> + Send>>,
        state: S,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
    where
        S: Clone + Send + Sync + 'static;

    /// Ajoute un handler avec état et accès complet à la requête.
    ///
    /// Utilisé pour les endpoints d'événements (SUBSCRIBE/UNSUBSCRIBE) qui nécessitent
    /// un accès aux en-têtes HTTP et à la méthode HTTP.
    ///
    /// # Arguments
    ///
    /// * `path` - Le chemin HTTP (ex: `/service/AVTransport/event`)
    /// * `handler` - Un pointeur de fonction avec accès complet à la requête
    /// * `state` - L'état partagé (typiquement une `ServiceInstance`)
    ///
    /// # Retour
    ///
    /// Une future qui se résout quand le handler est enregistré.
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmoupnp::{UpnpServer, server::{Response, HeaderMap, Request}};
    /// use axum::extract::State;
    /// use std::pin::Pin;
    /// use std::future::Future;
    ///
    /// fn event_handler(
    ///     State(service): State<String>,
    ///     headers: HeaderMap,
    ///     req: Request,
    /// ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
    ///     Box::pin(async move {
    ///         // Traiter SUBSCRIBE/UNSUBSCRIBE
    ///         axum::response::Response::default()
    ///     })
    /// }
    ///
    /// # async fn example<S: UpnpServer>(mut server: S) {
    /// server.add_handler_with_state(
    ///     "/event",
    ///     event_handler,
    ///     "ServiceName".to_string(),
    /// ).await;
    /// # }
    /// ```
    fn add_handler_with_state<S>(
        &mut self,
        path: &str,
        handler: fn(axum::extract::State<S>, HeaderMap, Request) -> Pin<Box<dyn Future<Output = Response> + Send>>,
        state: S,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
    where
        S: Clone + Send + Sync + 'static;
}
