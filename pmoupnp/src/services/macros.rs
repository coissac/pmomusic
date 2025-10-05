/// Macro pour définir facilement un service UPnP avec ses variables et actions.
///
/// Cette macro simplifie la création de services UPnP statiques en générant
/// automatiquement le code nécessaire pour initialiser un service avec ses
/// variables d'état et ses actions.
///
/// # Syntaxe
///
/// ```ignore
/// define_service! {
///     pub static SERVICE_NAME = "ServiceName" {
///         variables: [
///             VARIABLE1,
///             VARIABLE2,
///         ],
///         actions: [
///             ACTION1,
///             ACTION2,
///         ]
///     }
/// }
/// ```
///
/// # Arguments
///
/// - `SERVICE_NAME` : Nom de la constante statique Rust
/// - `"ServiceName"` : Nom du service UPnP (chaîne littérale)
/// - `variables:` : Section listant les références aux variables d'état
/// - `actions:` : Section listant les références aux actions
///
/// # Type de retour
///
/// La macro génère une `Lazy<Arc<Service>>` qui sera initialisée lors du premier accès.
///
/// # Prérequis
///
/// Les variables et actions référencées doivent être définies comme `Lazy<Arc<T>>`.
///
/// # Examples
///
/// ```ignore
/// use once_cell::sync::Lazy;
/// use std::sync::Arc;
///
/// // Définir les variables et actions ailleurs
/// pub static TRANSPORT_STATE: Lazy<Arc<StateVariable>> = ...;
/// pub static PLAY: Lazy<Arc<Action>> = ...;
/// pub static STOP: Lazy<Arc<Action>> = ...;
///
/// // Définir le service
/// define_service! {
///     pub static AVTRANSPORT = "AVTransport" {
///         variables: [
///             TRANSPORT_STATE,
///             TRANSPORT_URI,
///         ],
///         actions: [
///             PLAY,
///             STOP,
///             PAUSE,
///         ]
///     }
/// }
///
/// // Utilisation
/// fn main() {
///     let service = &*AVTRANSPORT;
///     println!("Service: {}", service.name());
/// }
/// ```
///
/// # Notes d'implémentation
///
/// - Les `Arc<StateVariable>` et `Arc<Action>` sont clonés
/// - Le service est wrappé dans un `Arc`
/// - Initialisation paresseuse via `Lazy` (thread-safe)
/// - Utilise `.expect()` pour les erreurs d'ajout
#[macro_export]
macro_rules! define_service {
    (pub static $name:ident = $service_name:literal {
        variables: [
            $($var:expr),* $(,)?
        ],
        actions: [
            $($action:expr),* $(,)?
        ]
    }) => {
        pub static $name: once_cell::sync::Lazy<std::sync::Arc<$crate::services::Service>> =
            once_cell::sync::Lazy::new(|| {
                use $crate::UpnpTyped;

                let mut svc = $crate::services::Service::new($service_name.to_string());

                $(
                    svc.add_variable(std::sync::Arc::clone(&*$var))
                        .expect(&format!("Cannot add variable {} to service {}",
                            (*$var).get_name(), svc.name()));
                )*

                $(
                    svc.add_action(std::sync::Arc::clone(&*$action))
                        .expect(&format!("Cannot add action {} to service {}",
                            (*$action).get_name(), svc.name()));
                )*

                std::sync::Arc::new(svc)
            });
    };
}
