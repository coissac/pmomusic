use std::sync::Arc;

use tracing::{debug, info, trace};
use xmltree::{Element, XMLNode};

use crate::{
    action_handler,
    UpnpModel,
    UpnpObject,
    UpnpObjectSetError,
    UpnpObjectType,
    UpnpTyped,
};
use crate::actions::{
    Action,
    ActionHandler,
    ActionInstance,
    Argument,
    ArgumentSet,
};

impl UpnpObject for Action {
    fn to_xml_element(&self) -> Element {
        let mut action_elem = Element::new("action");

        // <name>
        let mut name_elem = Element::new("name");
        name_elem
            .children
            .push(XMLNode::Text(self.get_name().clone()));
        action_elem.children.push(XMLNode::Element(name_elem));

        // <argumentList>
        let args_elem = self.arguments.to_xml_element();
        action_elem.children.push(XMLNode::Element(args_elem));

        action_elem
    }
}

impl UpnpModel for Action {
    type Instance = ActionInstance;
}

impl UpnpTyped for Action {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        return &self.object;
    }
}

impl Action {
    /// Crée un handler par défaut pour une action.
    ///
    /// Ce handler logge simplement l'appel et les arguments d'entrée.
    /// La méthode [`ActionInstance::run()`](crate::actions::ActionInstance::run) s'occupe
    /// automatiquement de :
    /// 1. Stocker les valeurs IN dans les variables liées avant d'appeler le handler
    /// 2. Collecter les valeurs OUT après l'exécution
    ///
    /// # Returns
    ///
    /// Un [`ActionHandler`] qui logge les entrées.
    ///
    /// # Comportement
    ///
    /// - Logge le nom de l'action
    /// - Logge les arguments IN avec leurs valeurs (lues depuis les variables liées)
    /// - Ne fait aucune modification (handler passif)
    ///
    /// # Note
    ///
    /// Ce handler est automatiquement assigné lors de la création d'une action.
    /// Il peut être remplacé via [`set_handler`](Self::set_handler).
    fn default_handler() -> ActionHandler {
        action_handler!(|instance| {
            use crate::UpnpTypedInstance;

            info!("🎬 Action '{}' called", instance.get_name());

            // Logger les arguments d'entrée (déjà stockés dans les variables par run())
            for arg_inst in instance.arguments_set().all() {
                let arg_model = arg_inst.as_ref().get_model();
                if arg_model.is_in() {
                    if let Some(var_inst) = arg_inst.get_variable_instance() {
                        trace!("  IN  {} = {:?}", arg_inst.get_name(), var_inst.value());
                    }
                }
            }

            Ok(()) // Succès - handler par défaut ne fait rien d'autre
        })
    }

    /// Crée une nouvelle action UPnP.
    ///
    /// L'action est initialisée avec un handler par défaut qui logge les entrées
    /// et retourne les valeurs des variables d'instance pour les arguments de sortie.
    ///
    /// # Arguments
    ///
    /// * `name` - Nom de l'action
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::actions::Action;
    /// let mut action = Action::new("Play".to_string());
    /// ```
    pub fn new(name: String) -> Action {
        Self {
            object: UpnpObjectType {
                name,
                object_type: "Action".to_string(),
            },
            arguments: ArgumentSet::new(),
            handle: Self::default_handler(),
        }
    }

    /// Ajoute un argument à l'action.
    ///
    /// # Arguments
    ///
    /// * `arg` - Argument à ajouter
    ///
    /// # Errors
    ///
    /// Retourne une erreur si un argument avec le même nom existe déjà.
    pub fn add_argument(&mut self, arg: Arc<Argument>) -> Result<(), UpnpObjectSetError> {
        self.arguments.insert(arg)
    }

    /// Retourne les arguments de l'action.
    pub fn arguments(&self) -> &ArgumentSet {
        &self.arguments
    }

    /// Définit un handler personnalisé pour cette action.
    ///
    /// Remplace le handler par défaut par un handler personnalisé.
    ///
    /// # Arguments
    ///
    /// * `handler` - Le nouveau handler à utiliser
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use pmoupnp::actions::{Action, ActionError};
    /// # use pmoupnp::action_handler;
    /// let mut action = Action::new("Play".to_string());
    ///
    /// let custom_handler = action_handler!(|instance, data| {
    ///     // Logique personnalisée
    ///     Ok::<(), ActionError>(())
    /// });
    ///
    /// action.set_handler(custom_handler);
    /// ```
    pub fn set_handler(&mut self, handler: ActionHandler) {
        self.handle = handler;
    }

    /// Retourne le handler de l'action.
    pub fn handler(&self) -> &ActionHandler {
        &self.handle
    }
}
