mod errors;

mod action_handler;
mod action_instance;
mod action_instance_set;
mod action_methods;
mod action_set_methods;
mod arg_inst_set_methods;
mod arg_instance_methods;
mod arg_set_methods;
mod argument_methods;
mod handler_helpers;

mod macros;

use crate::{
    UpnpObjectSet, UpnpObjectType,
    state_variables::{StateVarInstance, StateVariable},
};
use std::sync::{Arc, RwLock};

pub use action_handler::{ActionData, ActionFuture, ActionHandler};
pub use errors::ActionError;
pub use handler_helpers::{get_value, reflect_to_string, set_value};

/// Action UPnP.
///
/// Représente une opération invocable sur un service UPnP avec ses arguments
/// et son handler d'exécution.
///
/// # Structure
///
/// - **Arguments** : Liste d'arguments d'entrée (IN) et de sortie (OUT)
/// - **Handler** : Fonction asynchrone qui exécute l'action
///
/// # Handler par défaut
///
/// Chaque action est créée avec un handler par défaut qui :
/// - Logge les valeurs des arguments d'entrée
/// - Retourne les valeurs par défaut des arguments de sortie
///
/// # Examples
///
/// ```rust
/// use pmoupnp::actions::Action;
/// use pmoupnp::actions::Argument;
/// use pmoupnp::state_variables::StateVariable;
/// use pmoupnp::variable_types::StateVarType;
/// use std::sync::Arc;
///
/// let mut action = Action::new("Play".to_string());
///
/// // Ajouter des arguments
/// let instance_id = Arc::new(StateVariable::new(
///     StateVarType::UI4,
///     "InstanceID".to_string()
/// ));
/// let arg = Arc::new(Argument::new_in("InstanceID".to_string(), instance_id));
/// action.add_argument(arg);
/// ```
#[derive(Clone)]
pub struct Action {
    object: UpnpObjectType,
    arguments: ArgumentSet,
    handle: ActionHandler,
    stateful: bool,
}

impl std::fmt::Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Action")
            .field("object", &self.object)
            .field("arguments", &self.arguments)
            .field("handle", &"<ActionHandler>")
            .finish()
    }
}

pub type ActionSet = UpnpObjectSet<Action>;

#[derive(Debug, Clone)]
pub struct ActionInstance {
    object: UpnpObjectType,
    model: Action,
    arguments: ArgInstanceSet,
}

pub type ActionInstanceSet = UpnpObjectSet<ActionInstance>;

#[derive(Debug, Clone)]
pub struct Argument {
    object: UpnpObjectType,
    state_variable: Arc<StateVariable>,
    is_in: bool,
    is_out: bool,
}

pub type ArgumentSet = UpnpObjectSet<Argument>;

/// Instance d'un argument d'action UPnP.
///
/// Un `ArgumentInstance` représente un argument concret utilisé lors de l'exécution
/// d'une action. Contrairement au modèle [`Argument`] qui définit la structure,
/// l'instance maintient une liaison dynamique vers une [`StateVarInstance`] qui
/// contient la valeur runtime.
///
/// # Cycle de vie
///
/// 1. **Création** : Instanciation via [`UpnpInstance::new`] avec `variable_instance = None`
/// 2. **Liaison** : Association à une [`StateVarInstance`] via [`bind_variable`](Self::bind_variable)
/// 3. **Utilisation** : Accès à la valeur runtime via [`get_variable_instance`](Self::get_variable_instance)
/// 4. **Exécution** : Les valeurs IN sont stockées dans les variables liées lors de l'appel à `run()`
///
/// # Pourquoi `variable_instance` est optionnel ?
///
/// La liaison ne peut pas être faite dans le constructeur car :
/// - Les `StateVarInstance` sont créées **après** les modèles
/// - Les `ActionInstance` sont créées **avant** que toutes les variables soient disponibles
/// - La validation des dépendances se fait en deux phases
///
/// # Stockage des valeurs IN
///
/// Lors de l'exécution d'une action, les valeurs des arguments IN sont automatiquement
/// stockées dans les `StateVarInstance` liées. Les handlers peuvent ensuite y accéder
/// via `argument.get_variable_instance().value()` sans avoir besoin de recevoir les
/// valeurs en paramètre.
///
/// # Thread-safety
///
/// Le champ `variable_instance` est protégé par un `RwLock` pour permettre :
/// - La liaison après création (write lock)
/// - L'accès concurrent en lecture (read lock)
/// - L'utilisation dans un contexte multi-thread
///
/// # Examples
///
/// ```ignore
/// use pmoupnp::actions::{Argument, ArgumentInstance};
/// use pmoupnp::state_variables::StateVarInstance;
/// use std::sync::Arc;
///
/// // Phase 1 : Créer l'instance (sans liaison)
/// let arg_model = Argument::new_in("Volume".to_string(), volume_var);
/// let arg_instance = ArgumentInstance::new(&arg_model);
/// assert!(arg_instance.get_variable_instance().is_none());
///
/// // Phase 2 : Lier à une variable d'état
/// let var_instance = Arc::new(StateVarInstance::new(&volume_var));
/// arg_instance.bind_variable(var_instance.clone());
/// assert!(arg_instance.get_variable_instance().is_some());
///
/// // Phase 3 : Utiliser la valeur runtime
/// if let Some(var) = arg_instance.get_variable_instance() {
///     println!("Current value: {}", var.value());
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ArgumentInstance {
    /// Métadonnées de l'objet UPnP
    object: UpnpObjectType,

    /// Référence vers le modèle définissant la structure
    model: Argument,

    /// Liaison optionnelle vers l'instance de variable d'état.
    ///
    /// - `None` : Pas encore liée (état initial après construction)
    /// - `Some(Arc<StateVarInstance>)` : Liée et prête à l'emploi
    ///
    /// Protégée par `RwLock` pour permettre la liaison post-construction
    /// et l'accès concurrent en lecture.
    variable_instance: Arc<RwLock<Option<Arc<StateVarInstance>>>>,
}

pub type ArgInstanceSet = UpnpObjectSet<ArgumentInstance>;
