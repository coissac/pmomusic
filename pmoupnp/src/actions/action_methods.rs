use std::sync::Arc;

use tracing::{info, trace};
use xmltree::{Element, XMLNode};

use crate::actions::{Action, ActionHandler, ActionInstance, Argument, ArgumentSet};
use crate::{UpnpModel, UpnpObject, UpnpObjectSetError, UpnpObjectType, UpnpTyped, action_handler};

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
    /// Ce handler logge simplement l'appel et les arguments.
    ///
    /// # Returns
    ///
    /// Un [`ActionHandler`] qui logge les entrées et retourne les données telles quelles.
    ///
    /// # Comportement
    ///
    /// - Logge les arguments avec leurs valeurs
    /// - Ne fait aucune modification (handler passif)
    /// - Retourne les données telles quelles
    ///
    /// # Note
    ///
    /// Ce handler est automatiquement assigné lors de la création d'une action.
    /// Il peut être remplacé via [`set_handler`](Self::set_handler).
    fn default_handler() -> ActionHandler {
        action_handler!(|data| {
            info!("🎬 Action called with default handler");

            // Logger les arguments
            for (key, value) in data.iter() {
                trace!(
                    "  {} = {}",
                    key,
                    crate::actions::reflect_to_string(value.as_ref())
                );
            }

            // Retourner les données telles quelles
            Ok(data)
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
            stateful: true, // Par défaut, les actions sont stateful
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
    /// let custom_handler = action_handler!(|mut data| {
    ///     // Logique personnalisée
    ///     Ok::<_, ActionError>(data)
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

    /// Définit si l'action est stateful.
    ///
    /// Une action stateful met à jour les StateVarInstance lors de l'exécution,
    /// déclenchant ainsi les notifications d'événements UPnP.
    ///
    /// Une action stateless n'interagit pas avec les StateVarInstance,
    /// ce qui améliore les performances pour les opérations purement calculatoires.
    ///
    /// # Arguments
    ///
    /// * `stateful` - `true` pour stateful (défaut), `false` pour stateless
    ///
    /// # Returns
    ///
    /// `&mut Self` pour permettre le chaînage
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::actions::Action;
    /// let mut action = Action::new("Calculate".to_string());
    /// action.set_stateful(false);  // Action stateless
    /// ```
    pub fn set_stateful(&mut self, stateful: bool) -> &mut Self {
        self.stateful = stateful;
        self
    }

    pub fn set_stateless(&mut self, stateless: bool) -> &mut Self {
        self.stateful = !stateless;
        self
    }

    /// Retourne `true` si l'action est stateful.
    ///
    /// # Returns
    ///
    /// `true` si l'action met à jour les StateVarInstance (stateful),
    /// `false` si l'action est purement calculatoire (stateless).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::actions::Action;
    /// let mut action = Action::new("Play".to_string());
    /// assert!(action.is_stateful());  // Stateful par défaut
    ///
    /// action.set_stateful(false);
    /// assert!(!action.is_stateful());  // Maintenant stateless
    /// ```
    pub fn is_stateful(&self) -> bool {
        self.stateful
    }
}
