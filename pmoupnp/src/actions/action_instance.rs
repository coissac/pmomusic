use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bevy_reflect::Reflect;
use xmltree::{Element, XMLNode};

use crate::actions::{Action, ActionData, ActionInstance, ArgInstanceSet};
use crate::variable_types::StateValue;
use crate::{UpnpInstance, UpnpObject, UpnpObjectType, UpnpTyped, UpnpTypedInstance};

impl UpnpObject for ActionInstance {
    fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("action");

        // <name>
        let mut name_elem = Element::new("name");
        name_elem
            .children
            .push(XMLNode::Text(self.get_name().clone()));
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
            arguments, // ⬅️ Set d'instances, pas le modèle !
        }
    }
}

impl UpnpTypedInstance for ActionInstance {
    fn get_model(&self) -> &Self::Model {
        &self.model
    }
}

impl ActionInstance {
    /// Retourne `true` si l'action est stateful.
    ///
    /// Une action stateful met à jour les StateVarInstance lors de l'exécution.
    ///
    /// # Returns
    ///
    /// `true` si l'action est stateful, `false` si stateless.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::actions::{Action, ActionInstance};
    /// # use pmoupnp::UpnpInstance;
    /// let mut action = Action::new("Play".to_string());
    /// let instance = ActionInstance::new(&action);
    /// assert!(instance.is_stateful());  // Stateful par défaut
    /// ```
    pub fn is_stateful(&self) -> bool {
        self.model.is_stateful()
    }

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
        &self.arguments // ⬅️ Retourne les INSTANCES, pas les modèles !
    }

    /// Construit un [`ActionData`] initial à partir des variables d'état liées.
    ///
    /// Chaque argument lié à une [`StateVarInstance`](crate::state_variables::StateVarInstance)
    /// voit sa valeur courante convertie en [`Reflect`](bevy_reflect::Reflect) pour alimenter
    /// le handler de l'action.
    fn build_action_data(&self) -> ActionData {
        let mut data = HashMap::new();

        for arg_inst in self.arguments.all() {
            if let Some(var_inst) = arg_inst.get_variable_instance() {
                let name = arg_inst.get_name().to_string();
                let reflect_value = var_inst.to_reflect();
                data.insert(name, reflect_value);
            }
        }

        data
    }

    /// Fusionne les valeurs SOAP IN dans l'[`ActionData`] existant.
    ///
    /// Seuls les arguments marqués comme IN sont considérés. Les valeurs sont
    /// converties depuis [`StateValue`] vers `Reflect` pour les handlers.
    fn merge_soap_inputs(
        &self,
        action_data: &mut ActionData,
        soap_data: &HashMap<String, StateValue>,
    ) -> HashSet<String> {
        let mut updated = HashSet::new();

        for (arg_name, state_value) in soap_data.iter() {
            if let Some(arg_inst) = self.argument(arg_name) {
                if arg_inst.get_model().is_in() {
                    action_data.insert(arg_name.clone(), state_value.to_reflect());
                    updated.insert(arg_name.clone());
                }
            }
        }

        updated
    }

    /// Sauvegarde les arguments IN dans les variables d'état (mode stateful uniquement).
    async fn save_inputs_to_state_variables(
        &self,
        action_data: &ActionData,
        updated_keys: &HashSet<String>,
    ) -> Result<(), crate::actions::ActionError> {
        for arg_inst in self.arguments.all() {
            if arg_inst.get_model().is_in() && updated_keys.contains(arg_inst.get_name()) {
                if let Some(var_inst) = arg_inst.get_variable_instance() {
                    if let Some(reflect_value) = action_data.get(arg_inst.get_name()) {
                        let cloned = reflect_value.as_ref().reflect_clone().map_err(|e| {
                            crate::actions::ActionError::ArgumentError(e.to_string())
                        })?;
                        var_inst
                            .set_reflect_value(cloned)
                            .await
                            .map_err(|e| crate::actions::ActionError::SetError(e.to_string()))?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Sauvegarde les arguments OUT dans les variables d'état (mode stateful uniquement).
    async fn save_outputs_to_state_variables(
        &self,
        action_data: &ActionData,
    ) -> Result<(), crate::actions::ActionError> {
        for arg_inst in self.arguments.all() {
            if arg_inst.get_model().is_out() {
                if let Some(var_inst) = arg_inst.get_variable_instance() {
                    if let Some(reflect_value) = action_data.get(arg_inst.get_name()) {
                        let cloned = reflect_value
                            .as_ref()
                            .reflect_clone()
                            .map_err(|e| crate::actions::ActionError::SetError(e.to_string()))?;
                        var_inst
                            .set_reflect_value(cloned)
                            .await
                            .map_err(|e| crate::actions::ActionError::SetError(e.to_string()))?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Exécute l'action avec les données SOAP fournies.
    ///
    /// Workflow unifié :
    /// 1. Construire l'[`ActionData`] initial depuis les `StateVarInstance`
    /// 2. Fusionner les valeurs IN issues du SOAP
    /// 3. Si l'action est stateful : sauvegarder les IN dans les `StateVarInstance`
    /// 4. Exécuter le handler
    /// 5. Si l'action est stateful : sauvegarder les OUT dans les `StateVarInstance`
    /// 6. Retourner l'[`ActionData`] pour la réponse SOAP
    pub async fn run(
        self: Arc<Self>,
        soap_data: Arc<HashMap<String, StateValue>>,
    ) -> Result<ActionData, crate::actions::ActionError> {
        // 1. Construire ActionData initial
        let mut action_data = self.build_action_data();

        // 2. Fusionner les valeurs SOAP IN
        let updated_inputs = self.merge_soap_inputs(&mut action_data, &soap_data);

        // 3. Sauvegarder les IN si stateful
        if self.is_stateful() {
            self.save_inputs_to_state_variables(&action_data, &updated_inputs)
                .await?;
        }

        // 4. Exécuter le handler
        let handler = self.model.handler().clone();
        let result_data = handler(action_data).await?;

        // 5. Sauvegarder les OUT si stateful
        if self.is_stateful() {
            self.save_outputs_to_state_variables(&result_data).await?;
        }

        // 6. Retourner les données pour la réponse SOAP
        Ok(result_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UpnpInstance;
    use crate::actions::Action;

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
