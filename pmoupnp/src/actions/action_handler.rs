//! Types et utilitaires pour les handlers d'actions UPnP.
//!
//! Ce module définit les types fondamentaux pour gérer l'exécution
//! asynchrone des actions UPnP.
//!
//! # Architecture
//!
//! Les actions UPnP sont exécutées de manière asynchrone via des handlers
//! qui prennent des données en entrée et retournent des données en sortie.
//!
//! ```text
//! ActionData (input)
//!       ↓
//! ActionHandler (async processing)
//!       ↓
//! ActionData (output)
//! ```
//!
//! # Examples
//!
//! ```rust
//! use pmoupnp::action_handler;
//! use pmoupnp::actions::ActionError;
//! use std::collections::HashMap;
//! use std::sync::Arc;
//!
//! // Créer un handler avec la macro
//! let handler = action_handler!(|instance, data| {
//!     // Traiter les données
//!     Ok::<(), ActionError>(())
//! });
//!
//! // Ou manuellement
//! use pmoupnp::actions::{ActionData, ActionHandler, ActionInstance};
//! let manual_handler: ActionHandler = Arc::new(|instance, data| {
//!     Box::pin(async move {
//!         Ok::<(), ActionError>(())
//!     })
//! });
//! ```

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use crate::variable_types::StateValue;

/// Données d'une action UPnP.
///
/// Représente un ensemble de paramètres clé-valeur pour une action UPnP,
/// partagé via `Arc` pour permettre un clonage efficace.
///
/// # Structure
///
/// - **Clé** : Nom du paramètre (ex: "InstanceID", "TransportURI")
/// - **Valeur** : Valeur typée du paramètre ([`StateValue`])
///
/// # Exemples
///
/// ```rust
/// use pmoupnp::actions::ActionData;
/// use pmoupnp::variable_types::StateValue;
/// use std::collections::HashMap;
/// use std::sync::Arc;
///
/// let mut data = HashMap::new();
/// data.insert("InstanceID".to_string(), StateValue::UI4(0));
/// data.insert("Speed".to_string(), StateValue::String("1".to_string()));
///
/// let action_data: ActionData = Arc::new(data);
///
/// // Le Arc permet un clonage efficace
/// let cloned = action_data.clone();
/// ```
///
/// # Notes
///
/// - Utilise `Arc` pour éviter les copies coûteuses
/// - Thread-safe grâce à `Arc`
/// - Les valeurs sont immuables une fois créées
pub type ActionData = Arc<HashMap<String, StateValue>>;

/// Future retourné par un [`ActionHandler`].
///
/// Ce type représente le résultat asynchrone d'un handler d'action.
/// Il est boxé et pinné pour permettre le polymorphisme et la manipulation
/// sûre des futures.
///
/// # Type complet
///
/// ```ignore
/// Pin<Box<dyn Future<Output = Result<(), ActionError>> + Send>>
/// ```
///
/// # Composants
///
/// - `Pin<Box<...>>` : Permet de déplacer le future en mémoire sans invalidation
/// - `dyn Future<Output = Result<(), ActionError>>` : Future retournant un Result
/// - `+ Send` : Le future peut être envoyé entre threads
///
/// # Notes
///
/// - Les handlers retournent `Ok(())` en cas de succès ou `Err(ActionError)` en cas d'erreur
/// - Ils modifient les variables d'instance et [`ActionInstance::run()`](crate::actions::ActionInstance::run)
///   collecte automatiquement les valeurs OUT si le handler réussit
/// - Rarement utilisé directement (la macro `action_handler!` s'en charge)
/// - Nécessaire pour la compatibilité avec les trait objects
pub type ActionFuture = Pin<Box<dyn Future<Output = Result<(), crate::actions::ActionError>> + Send>>;

/// Handler d'action UPnP asynchrone.
///
/// Un `ActionHandler` est une fonction asynchrone partageable qui exécute
/// la logique métier d'une action sans retourner de valeur.
///
/// # Signature
///
/// ```ignore
/// Fn(Arc<ActionInstance>, ActionData) -> ActionFuture
/// ```
///
/// Prend :
/// - [`Arc<ActionInstance>`](crate::actions::ActionInstance) : L'instance de l'action avec accès aux variables liées
/// - [`ActionData`] : Les données d'entrée (arguments IN)
///
/// Retourne un [`ActionFuture`] qui se résout en `Result<(), ActionError>`.
///
/// # Responsabilités
///
/// Le handler est responsable de :
/// - Lire les arguments d'entrée depuis `data`
/// - Exécuter la logique métier
/// - Modifier les variables d'instance selon les besoins
/// - Retourner `Ok(())` en cas de succès ou `Err(ActionError)` en cas d'erreur
///
/// La méthode [`ActionInstance::run()`](crate::actions::ActionInstance::run) s'occupe
/// automatiquement de collecter les valeurs OUT si le handler retourne `Ok(())`.
///
/// # Traits requis
///
/// - `Send` : Le handler peut être envoyé entre threads
/// - `Sync` : Le handler peut être partagé entre threads
/// - `Arc` : Permet le partage sans copie
///
/// # Création
///
/// ## Avec la macro (recommandé)
///
/// ```rust
/// use pmoupnp::action_handler;
/// use pmoupnp::actions::ActionError;
///
/// let handler = action_handler!(|instance, data| {
///     // Logique métier
///     Ok::<(), ActionError>(())
/// });
/// ```
///
/// ## Manuellement
///
/// ```rust
/// use pmoupnp::actions::{ActionData, ActionHandler, ActionInstance, ActionError};
/// use std::sync::Arc;
///
/// let handler: ActionHandler = Arc::new(|instance, data| {
///     Box::pin(async move {
///         // Votre logique async
///         Ok::<(), ActionError>(())
///     })
/// });
/// ```
///
/// # Notes d'implémentation
///
/// - Le handler ne retourne rien - il modifie les variables d'instance
/// - [`ActionInstance::run()`](crate::actions::ActionInstance::run) collecte automatiquement les OUT
/// - Le handler capture les variables par `move`
/// - Le future est automatiquement `Send` si les captures le sont
/// - Utilisez la macro `action_handler!` pour simplifier la création
pub type ActionHandler = Arc<dyn Fn(Arc<crate::actions::ActionInstance>, ActionData) -> ActionFuture + Send + Sync>;

/// Macro pour créer facilement un ActionHandler.
///
/// Cette macro simplifie la création d'handlers asynchrones en cachant
/// la complexité de `Arc`, `Box::pin`, et `async move`.
///
/// # Syntaxe
///
/// ```ignore
/// action_handler!(|instance, data| {
///     // votre logique async (automatiquement dans un bloc async move)
///     data
/// })
/// ```
///
/// # Arguments
///
/// - `instance` : Paramètre de type `Arc<`[`ActionInstance`](crate::actions::ActionInstance)`>` - L'instance de l'action
/// - `data` : Paramètre de type [`ActionData`] (Arc<HashMap<String, StateValue>>) - Les données d'entrée
/// - Le corps du bloc peut contenir du code asynchrone (`.await`)
///
/// # Type de retour
///
/// La macro retourne un [`ActionHandler`] prêt à l'emploi.
///
/// # Examples
///
/// ## Exemple 1 : Handler simple (ne fait rien)
///
/// ```ignore
/// use pmoupnp::action_handler;
///
/// // Handler minimal - run() collectera automatiquement les OUT
/// let handler = action_handler!(|instance, data| {
///     Ok(()) // Succès, pas d'erreur
/// });
/// ```
///
/// ## Exemple 2 : Handler qui modifie une variable avec gestion d'erreur
///
/// ```ignore
/// use pmoupnp::action_handler;
/// use pmoupnp::actions::ActionError;
///
/// let handler = action_handler!(|instance, data| {
///     // Lire un argument d'entrée
///     let volume = data.get("DesiredVolume")
///         .ok_or_else(|| ActionError::MissingArgument("DesiredVolume".to_string()))?;
///
///     // Modifier la variable d'instance
///     let arg = instance.argument("CurrentVolume")
///         .ok_or_else(|| ActionError::ArgumentNotFound("CurrentVolume".to_string()))?;
///
///     let var = arg.get_variable_instance()
///         .ok_or_else(|| ActionError::VariableNotBound)?;
///
///     var.set_value(volume.clone());
///
///     Ok(()) // Succès - run() collectera CurrentVolume dans les OUT
/// });
/// ```
///
/// ## Exemple 3 : Handler avec logique métier asynchrone et gestion d'erreur
///
/// ```ignore
/// use pmoupnp::action_handler;
/// use pmoupnp::actions::ActionError;
///
/// let handler = action_handler!(|instance, data| {
///     // Appel asynchrone à un service externe
///     let response = external_service::fetch_data().await
///         .map_err(|e| ActionError::ExternalError(e.to_string()))?;
///
///     // Mettre à jour les variables selon la réponse
///     if let Some(arg) = instance.argument("Status") {
///         if let Some(var) = arg.get_variable_instance() {
///             var.set_value(StateValue::String(response.status));
///         }
///     }
///
///     if let Some(arg) = instance.argument("Message") {
///         if let Some(var) = arg.get_variable_instance() {
///             var.set_value(StateValue::String(response.message));
///         }
///     }
///
///     Ok(())
/// });
/// ```
///
/// ## Exemple 4 : Handler avec capture de contexte et validation
///
/// ```ignore
/// use pmoupnp::action_handler;
/// use pmoupnp::actions::ActionError;
/// use std::sync::Arc;
/// use tokio::sync::Mutex;
///
/// // Contexte partagé (ex: état d'un lecteur média)
/// let player_state = Arc::new(Mutex::new(PlayerState::Stopped));
///
/// let handler = action_handler!(|instance, data| {
///     // Vérifier l'état actuel
///     {
///         let state = player_state.lock().await;
///         if *state == PlayerState::Error {
///             return Err(ActionError::InvalidState("Player in error state".to_string()));
///         }
///     }
///
///     // Modifier l'état du lecteur
///     {
///         let mut state = player_state.lock().await;
///         *state = PlayerState::Playing;
///     }
///
///     // Mettre à jour la variable TransportState
///     if let Some(arg) = instance.argument("CurrentTransportState") {
///         if let Some(var) = arg.get_variable_instance() {
///             var.set_value(StateValue::String("PLAYING".to_string()));
///         }
///     }
///
///     Ok(())
/// });
/// ```
///
/// # Notes d'implémentation
///
/// - Le bloc est automatiquement wrappé dans `async move`
/// - Les captures de variables sont déplacées (`move`)
/// - Le résultat est automatiquement boxé et arcé
#[macro_export]
macro_rules! action_handler {
    (|$instance:ident, $data:ident| $body:block) => {
        std::sync::Arc::new(|$instance: std::sync::Arc<$crate::actions::ActionInstance>, $data: $crate::actions::ActionData| {
            Box::pin(async move $body)
        })
    };
}