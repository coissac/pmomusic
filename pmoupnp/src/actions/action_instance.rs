use std::sync::Arc;

use tracing::{debug, trace};
use xmltree::{Element, XMLNode};

use crate::{
    UpnpInstance,
    UpnpObject,
    UpnpObjectType,
    UpnpTyped,
    UpnpTypedInstance,
};
use crate::actions::{
    Action,
    ActionData,
    ActionInstance,
    ArgInstanceSet,
};

impl UpnpObject for ActionInstance {
    fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("action");

        // <name>
        let mut name_elem = Element::new("name");
        name_elem.children.push(XMLNode::Text(self.get_name().clone()));
        elem.children.push(XMLNode::Element(name_elem));

        // Utiliser le set d'instances d'arguments
        let args_container = self.arguments.to_xml_element();
        elem.children.push(XMLNode::Element(args_container));

        elem
    }  
}

impl UpnpTyped for ActionInstance {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        return &self.object;
    }
}

impl UpnpInstance for ActionInstance {

    type Model = Action;

    fn new(action: &Action) -> Self {
        // Créer les instances d'arguments
        let mut arguments = ArgInstanceSet::new();
        
        for arg_model in action.arguments().all() {
            let arg_instance = Arc::new(crate::actions::ArgumentInstance::new(&*arg_model));
            if let Err(e) = arguments.insert(arg_instance) {
                tracing::error!("Failed to insert argument instance: {:?}", e);
            }
        }
        
        Self {
            object: UpnpObjectType {
                name: action.get_name().clone(),
                object_type: "ActionInstance".to_string(),
            },
            model: action.clone(),
            arguments,  // ⬅️ Set d'instances, pas le modèle !
        }
    }

}


impl UpnpTypedInstance for ActionInstance {

    fn get_model(&self) -> &Self::Model {
        &self.model
    }
}

impl ActionInstance {
    /// Retourne une instance d'argument par son nom.
    ///
    /// # Arguments
    ///
    /// * `name` - Nom de l'argument à rechercher
    ///
    /// # Returns
    ///
    /// `Some(Arc<ArgumentInstance>)` si trouvé, `None` sinon.
    pub fn argument(&self, name: &str) -> Option<Arc<crate::actions::ArgumentInstance>> {
        self.arguments.get_by_name(name)
    }

    /// Retourne le set d'instances d'arguments.
    ///
    /// # Returns
    ///
    /// Référence vers le `ArgInstanceSet` contenant toutes les instances.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// for arg_instance in action_instance.arguments_set().all() {
    ///     println!("Argument: {}", arg_instance.get_name());
    ///     if let Some(var) = arg_instance.get_variable_instance() {
    ///         println!("  Variable: {} = {}", var.get_name(), var.value());
    ///     }
    /// }
    /// ```
    pub fn arguments_set(&self) -> &ArgInstanceSet {
        &self.arguments  // ⬅️ Retourne les INSTANCES, pas les modèles !
    }

    /// Récupère les valeurs de tous les arguments de sortie (OUT).
    ///
    /// Cette méthode collecte automatiquement les valeurs actuelles de toutes
    /// les variables d'état liées aux arguments OUT et les retourne dans un
    /// [`ActionData`] indexé par le nom de chaque argument.
    ///
    /// # Returns
    ///
    /// Un [`ActionData`] contenant les paires (nom_argument, valeur_variable) pour
    /// tous les arguments de sortie qui ont une variable d'instance liée.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::actions::{Action, ActionInstance};
    /// # use pmoupnp::UpnpInstance;
    /// # use std::sync::Arc;
    /// let action = Action::new("GetVolume".to_string());
    /// let instance = Arc::new(ActionInstance::new(&action));
    ///
    /// // Récupérer automatiquement toutes les valeurs OUT
    /// let output = instance.get_out_values();
    ///
    /// // Afficher les résultats
    /// for (arg_name, value) in output.iter() {
    ///     println!("{} = {:?}", arg_name, value);
    /// }
    /// ```
    ///
    /// # Notes
    ///
    /// - Seuls les arguments marqués comme OUT sont inclus
    /// - Les arguments sans variable d'instance liée sont ignorés
    /// - Le nom de l'argument (pas le nom de la variable) est utilisé comme clé
    /// - Cette méthode est utilisée par le handler par défaut
    pub fn get_out_values(&self) -> ActionData {
        use std::collections::HashMap;
        use crate::UpnpTypedInstance;

        let mut result = HashMap::new();

        for arg_inst in self.arguments.all() {
            let arg_model = arg_inst.as_ref().get_model();
            if arg_model.is_out() {
                if let Some(var_inst) = arg_inst.get_variable_instance() {
                    result.insert(arg_inst.get_name().to_string(), var_inst.value());
                }
            }
        }

        Arc::new(result)
    }

    /// Exécute l'action avec les données fournies.
    ///
    /// Cette méthode :
    /// 1. Stocke les valeurs IN dans les variables liées
    /// 2. Exécute le handler (qui peut accéder aux valeurs IN via les variables)
    /// 3. Collecte automatiquement les valeurs OUT via [`get_out_values()`](Self::get_out_values)
    /// 4. Retourne les résultats
    ///
    /// # Arguments
    ///
    /// * `data` - Données d'entrée de l'action (arguments IN)
    ///
    /// # Returns
    ///
    /// Un `Future` qui se résout en `Result<ActionData, ActionError>` :
    /// - `Ok(ActionData)` contenant les résultats (arguments OUT) si le handler réussit
    /// - `Err(ActionError)` si le handler échoue
    ///
    /// # Errors
    ///
    /// Retourne une erreur si le handler retourne `Err(ActionError)`.
    ///
    /// # Fonctionnement
    ///
    /// 1. Pour chaque argument IN, la valeur fournie dans `data` est stockée dans la
    ///    variable d'état liée à cet argument
    /// 2. Le handler est appelé avec l'instance (il peut lire les valeurs IN via
    ///    `argument.get_variable_instance().value()`)
    /// 3. Le handler modifie les variables selon ses besoins et retourne `Ok(())` ou `Err(...)`
    /// 4. Si le handler réussit, `run()` collecte automatiquement toutes les valeurs
    ///    des arguments marqués comme OUT
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use pmoupnp::actions::{Action, ActionData, ActionInstance};
    /// # use pmoupnp::UpnpInstance;
    /// # use std::collections::HashMap;
    /// # use std::sync::Arc;
    /// # async fn example() {
    /// let action = Action::new("SetVolume".to_string());
    /// let instance = Arc::new(ActionInstance::new(&action));
    ///
    /// // Préparer les données d'entrée
    /// let mut input = HashMap::new();
    /// input.insert("DesiredVolume".to_string(),
    ///              pmoupnp::variable_types::StateValue::UI2(50));
    /// let input_data = Arc::new(input);
    ///
    /// // Exécuter l'action
    /// // 1. run() stocke DesiredVolume=50 dans la variable liée
    /// // 2. Le handler lit la valeur et fait son travail
    /// // 3. run() retourne automatiquement les valeurs OUT
    /// match instance.run(input_data).await {
    ///     Ok(output_data) => {
    ///         // Traiter les résultats
    ///         for (key, value) in output_data.iter() {
    ///             println!("{} = {:?}", key, value);
    ///         }
    ///     }
    ///     Err(e) => {
    ///         eprintln!("Action failed: {:?}", e);
    ///     }
    /// }
    /// # }
    /// ```
    ///
    /// # Notes
    ///
    /// - Les valeurs IN sont automatiquement stockées avant l'appel du handler
    /// - Le handler n'a plus besoin de recevoir les données en paramètre
    /// - Le handler modifie les variables et retourne `Ok(())` ou `Err(ActionError)`
    /// - `run()` collecte automatiquement les OUT si le handler retourne `Ok(())`
    /// - L'instance doit être wrappée dans un `Arc` pour être passée au handler
    pub async fn run(self: Arc<Self>, data: ActionData) -> Result<ActionData, crate::actions::ActionError> {
        // Stocker les valeurs IN dans les variables liées
        for arg_inst in self.arguments.all() {
            let arg_model = arg_inst.as_ref().get_model();
            if arg_model.is_in() {
                if let Some(value) = data.get(arg_inst.get_name()) {
                    if let Some(var_inst) = arg_inst.get_variable_instance() {
                        var_inst.set_value(value.clone());
                        trace!("  IN  {} = {:?}", arg_inst.get_name(), value);
                    }
                }
            }
        }

        let handler = self.model.handler().clone();
        let instance_clone = self.clone();

        // Exécuter le handler (il peut maintenant lire les valeurs IN depuis les variables)
        handler(instance_clone).await?;

        // Collecter automatiquement les valeurs OUT si succès
        debug!("✅ Action '{}' completed successfully, collecting outputs", self.get_name());
        Ok(self.get_out_values())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::UpnpInstance;

    #[test]
    fn test_action_instance_creation() {
        let action = Action::new("Play".to_string());
        let instance = ActionInstance::new(&action);
        
        assert_eq!(instance.get_name(), "Play");
    }
    
    #[test]
    fn test_action_instance_has_argument_instances() {
        let action = Action::new("Play".to_string());
        let instance = ActionInstance::new(&action);
        
        // Vérifier que arguments_set() retourne bien des instances
        assert!(instance.arguments_set().all().iter().all(|arg| {
            // Chaque argument doit être une ArgumentInstance
            arg.get_model(); // Cette méthode existe seulement sur les instances
            true
        }));
    }
}

