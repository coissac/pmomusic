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
//! let handler = action_handler!(|mut data| {
//!     // Traiter les données
//!     Ok::<_, ActionError>(data)
//! });
//!
//! // Ou manuellement
//! use pmoupnp::actions::{ActionData, ActionHandler};
//! let manual_handler: ActionHandler = Arc::new(|data| {
//!     Box::pin(async move { Ok::<_, ActionError>(data) })
//! });
//! ```

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use bevy_reflect::Reflect;

/// Données d'une action UPnP (entrée/sortie unifiées).
///
/// Représente un ensemble de paramètres clé-valeur pour une action UPnP,
/// utilisant des valeurs Reflect pour la flexibilité de typage.
///
/// # Structure
///
/// - **Clé** : Nom du paramètre (ex: "InstanceID", "TransportURI")
/// - **Valeur** : Valeur dynamique via `Box<dyn Reflect>`
///
/// # Exemples
///
/// ```rust
/// use pmoupnp::actions::ActionData;
/// use std::collections::HashMap;
/// use bevy_reflect::Reflect;
///
/// let mut data: ActionData = HashMap::new();
/// data.insert("InstanceID".to_string(), Box::new(0u32));
/// data.insert("Speed".to_string(), Box::new("1".to_string()));
///
/// // Les valeurs peuvent être modifiées
/// data.insert("InstanceID".to_string(), Box::new(1u32));
/// ```
///
/// # Notes
///
/// - Utilise `Box<dyn Reflect>` pour la flexibilité de typage
/// - Même type pour les entrées et sorties du handler
/// - Le handler peut modifier directement les données
pub type ActionData = HashMap<String, Box<dyn Reflect>>;

/// Future retourné par un [`ActionHandler`].
///
/// Ce type représente le résultat asynchrone d'un handler d'action.
/// Il est boxé et pinné pour permettre le polymorphisme et la manipulation
/// sûre des futures.
///
/// # Type complet
///
/// ```ignore
/// Pin<Box<dyn Future<Output = Result<ActionData, ActionError>> + Send>>
/// ```
///
/// # Composants
///
/// - `Pin<Box<...>>` : Permet de déplacer le future en mémoire sans invalidation
/// - `dyn Future<Output = Result<ActionData, ActionError>>` : Future retournant un Result avec les données modifiées
/// - `+ Send` : Le future peut être envoyé entre threads
///
/// # Notes
///
/// - Les handlers retournent `Ok(ActionData)` en cas de succès ou `Err(ActionError)` en cas d'erreur
/// - Le handler retourne les données modifiées (ActionData unifié pour entrée/sortie)
/// - Rarement utilisé directement (la macro `action_handler!` s'en charge)
/// - Nécessaire pour la compatibilité avec les trait objects
pub type ActionFuture =
    Pin<Box<dyn Future<Output = Result<ActionData, crate::actions::ActionError>> + Send>>;

/// Handler d'action UPnP asynchrone.
///
/// Un `ActionHandler` est une fonction asynchrone partageable qui exécute
/// la logique métier d'une action et retourne les données modifiées.
///
/// # Signature
///
/// ```ignore
/// Fn(ActionData) -> ActionFuture
/// ```
///
/// Prend :
/// - [`ActionData`] : HashMap contenant les valeurs des arguments (Box<dyn Reflect>)
///
/// Retourne un [`ActionFuture`] qui se résout en `Result<ActionData, ActionError>`.
///
/// # Responsabilités
///
/// Le handler est responsable de :
/// - Lire les arguments d'entrée depuis ActionData
/// - Exécuter la logique métier
/// - Modifier les données selon les besoins
/// - Retourner `Ok(ActionData)` avec les données modifiées ou `Err(ActionError)` en cas d'erreur
///
/// La méthode [`ActionInstance::run()`](crate::actions::ActionInstance::run) s'occupe
/// automatiquement de :
/// 1. Construire ActionData depuis les StateVarInstance
/// 2. Merger les valeurs IN du SOAP
/// 3. (Si stateful) Sauver les IN dans les StateVarInstance avant le handler
/// 4. Exécuter le handler
/// 5. (Si stateful) Sauver les OUT dans les StateVarInstance après le handler
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
/// let handler = action_handler!(|data| {
///     // Logique métier avec ActionData
///     Ok(data)
/// });
/// ```
///
/// ## Manuellement
///
/// ```rust
/// use pmoupnp::actions::{ActionHandler, ActionData, ActionError};
/// use std::sync::Arc;
///
/// let handler: ActionHandler = Arc::new(|data| {
///     Box::pin(async move {
///         // Votre logique async
///         Ok(data)
///     })
/// });
/// ```
///
/// # Notes d'implémentation
///
/// - Le handler reçoit et retourne ActionData (type unifié entrée/sortie)
/// - Le handler peut modifier directement les données reçues
/// - Le handler capture les variables par `move`
/// - Le future est automatiquement `Send` si les captures le sont
/// - Utilisez la macro `action_handler!` pour simplifier la création
pub type ActionHandler = Arc<dyn Fn(ActionData) -> ActionFuture + Send + Sync>;

/// Macro pour créer facilement un ActionHandler.
///
/// Cette macro simplifie la création d'handlers asynchrones en cachant
/// la complexité de `Arc`, `Box::pin`, et `async move`.
///
/// # Syntaxe
///
/// ```ignore
/// action_handler!(|data| {
///     // votre logique async avec ActionData
///     // Modifier les données et les retourner
///     Ok(data)
/// })
/// ```
///
/// # Arguments
///
/// - `data` : Paramètre de type [`ActionData`] - HashMap contenant les valeurs des arguments
/// - Le corps du bloc peut contenir du code asynchrone (`.await`)
///
/// # Type de retour
///
/// La macro retourne un [`ActionHandler`] prêt à l'emploi.
///
/// # Examples
///
/// ## Exemple 1 : Handler simple (retourne les données telles quelles)
///
/// ```ignore
/// use pmoupnp::action_handler;
///
/// let handler = action_handler!(|data| {
///     Ok(data) // Retourne les données non modifiées
/// });
/// ```
///
/// ## Exemple 2 : Handler qui calcule et modifie les données
///
/// ```ignore
/// use pmoupnp::{action_handler, get, set};
/// use pmoupnp::actions::ActionError;
///
/// let handler = action_handler!(|mut data| {
///     // Extraire les valeurs avec la macro get!
///     let celsius: f64 = get!(data, "Celsius", f64);
///
///     // Calculer
///     let fahrenheit = celsius * 9.0 / 5.0 + 32.0;
///
///     // Insérer avec la macro set!
///     set!(data, "Fahrenheit", fahrenheit);
///
///     Ok(data) // Retourner les données modifiées
/// });
/// ```
///
/// ## Exemple 3 : Handler avec logique métier asynchrone
///
/// ```ignore
/// use pmoupnp::{action_handler, get, set};
/// use pmoupnp::actions::ActionError;
///
/// let handler = action_handler!(|mut data| {
///     // Lire l'URI
///     let uri: String = get!(data, "URI", String);
///
///     // Appel asynchrone à un service externe
///     let metadata = external_service::fetch_metadata(&uri).await
///         .map_err(|e| ActionError::ExternalError(e.to_string()))?;
///
///     // Mettre à jour les données
///     set!(data, "Metadata", metadata);
///
///     Ok(data)
/// });
/// ```
///
/// ## Exemple 4 : Handler avec capture de contexte
///
/// ```ignore
/// use pmoupnp::{action_handler, get, set};
/// use pmoupnp::actions::ActionError;
/// use std::sync::Arc;
/// use tokio::sync::Mutex;
///
/// // Contexte partagé
/// let player_state = Arc::new(Mutex::new(PlayerState::Stopped));
///
/// let handler = action_handler!(|mut data| {
///     // Vérifier l'état
///     {
///         let state = player_state.lock().await;
///         if *state == PlayerState::Error {
///             return Err(ActionError::InvalidState("Player in error state".into()));
///         }
///     }
///
///     // Modifier l'état
///     {
///         let mut state = player_state.lock().await;
///         *state = PlayerState::Playing;
///     }
///
///     // Mettre à jour les données
///     set!(data, "TransportState", "PLAYING".to_string());
///
///     Ok(data)
/// });
/// ```
///
/// # Notes d'implémentation
///
/// - Le bloc est automatiquement wrappé dans `async move`
/// - Les captures de variables sont déplacées (`move`)
/// - Le résultat est automatiquement boxé et arcé
/// - Utilisez les macros `get!` et `set!` pour manipuler facilement les données
#[macro_export]
macro_rules! action_handler {
    (|$data:ident| $body:block) => {
        std::sync::Arc::new(|$data: $crate::actions::ActionData| {
            Box::pin(async move $body)
        })
    };
}
