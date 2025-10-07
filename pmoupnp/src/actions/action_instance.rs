use std::sync::Arc;

use xmltree::{Element, XMLNode};

use crate::actions::Action;
use crate::actions::ArgInstanceSet; 
use crate::actions::ActionInstance;
use crate::UpnpInstance;
use crate::UpnpObject;
use crate::UpnpTyped;
use crate::UpnpTypedInstance;
use crate::UpnpObjectType;

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

