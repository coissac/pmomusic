use std::{
    collections::HashMap,
    fmt,
    sync::Arc,
};

use tokio::sync::RwLock;
use xmltree::{Element, XMLNode};

use crate::{
    UpnpObjectType, UpnpTyped,
    object_trait::{UpnpModel, UpnpObject},
    state_variables::{
        StateConditionFunc, StateVarInstance, StateVariable, StringValueParser, ValueSerializer,
        variable_trait::UpnpVariable,
    },
    value_ranges::ValueRange,
    variable_types::{StateValue, StateValueError, StateVarType, UpnpVarType},
};

impl UpnpTyped for StateVariable {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        &self.object
    }
}

impl UpnpVarType for StateVariable {
    fn as_state_var_type(&self) -> StateVarType {
        self.value_type.as_state_var_type() // utilise ton From<&StateValue> existant
    }
}

impl UpnpObject for StateVariable {
    async fn to_xml_element(&self) -> Element {
        // Création de l'élément racine <stateVariable>
        let mut root = Element::new("stateVariable");
        root.attributes.insert(
            "sendEvents".to_string(),
            if self.send_events { "yes" } else { "no" }.to_string(),
        );

        // <name>
        let mut name_elem = Element::new("name");
        name_elem
            .children
            .push(XMLNode::Text(self.get_name().clone()));

        // <dataType>
        let mut datatype_elem = Element::new("dataType");
        datatype_elem
            .children
            .push(XMLNode::Text(self.value_type.to_string())); // StateVarType doit impl Display

        // <defaultValue> si défini
        if let Some(default) = &self.default_value {
            let mut def_elem = Element::new("defaultValue");
            def_elem.children.push(XMLNode::Text(default.to_string()));
            root.children.push(XMLNode::Element(def_elem));
        }

        // <allowedValueList> si défini
        let av = self.allowed_values.read().await;
        if !av.is_empty() {
            let mut list_elem = Element::new("allowedValueList");
            for val in av.iter() {
                let mut val_elem = Element::new("allowedValue");
                val_elem.children.push(XMLNode::Text(val.to_string()));
                list_elem.children.push(XMLNode::Element(val_elem));
            }
            root.children.push(XMLNode::Element(list_elem));
        }

        // <allowedValueRange> si défini
        if let Some(range) = &self.value_range {
            let mut range_elem = Element::new("allowedValueRange");

            let mut min_elem = Element::new("minimum");
            min_elem
                .children
                .push(XMLNode::Text(range.get_minimum().to_string()));
            range_elem.children.push(XMLNode::Element(min_elem));

            let mut max_elem = Element::new("maximum");
            max_elem
                .children
                .push(XMLNode::Text(range.get_maximum().to_string()));
            range_elem.children.push(XMLNode::Element(max_elem));

            if let Some(step) = &self.step {
                let mut step_elem = Element::new("step");
                step_elem.children.push(XMLNode::Text(step.to_string()));
                range_elem.children.push(XMLNode::Element(step_elem));
            }

            root.children.push(XMLNode::Element(range_elem));
        }

        // Ajouter les enfants communs
        root.children.push(XMLNode::Element(name_elem));
        root.children.push(XMLNode::Element(datatype_elem));

        root
    }
}

impl UpnpModel for StateVariable {
    type Instance = StateVarInstance;
}

impl Clone for StateVariable {
    fn clone(&self) -> Self {
        // clone safe des structures protégées par RwLock en prenant un read lock
        let event_conditions_clone = {
            // si le lock est "poisoned" on panic - tu peux adapter la gestion si tu veux
            let guard = self
                .event_conditions
                .blocking_read();
            // nécessite que Key: Clone, Value: Clone
            Arc::new(RwLock::new(guard.clone()))
        };

        let allowed_values_clone = {
            let guard = self
                .allowed_values
                .blocking_read();
            Arc::new(RwLock::new(guard.clone()))
        };

        Self {
            object: self.object.clone(),
            value_type: self.value_type.clone(),
            step: self.step.clone(),
            modifiable: self.modifiable,
            event_conditions: event_conditions_clone,
            description: self.description.clone(),
            default_value: self.default_value.clone(),
            value_range: self.value_range.clone(),
            allowed_values: allowed_values_clone,
            send_events: self.send_events,
            // parse et marshal sont typiquement des Arc<dyn ...> — on clone l'Arc (shallow).
            // Deep-cloner une closure ou un trait-objet n'est pas possible en général.
            parse: self.parse.clone(),
            marshal: self.marshal.clone(),
        }
    }
}

impl fmt::Debug for StateVariable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateVariable")
            .field("object", &self.object)
            .field("value_type", &self.value_type)
            .field("step", &self.step)
            .field("modifiable", &self.modifiable)
            .field(
                "event_conditions",
                &format_args!(
                    "len={}",
                    self.event_conditions.blocking_read().len()
                ),
            )
            .field("description", &self.description)
            .field("default_value", &self.default_value)
            .field("value_range", &self.value_range)
            .field(
                "allowed_values",
                &format_args!(
                    "len={}",
                    self.allowed_values.blocking_read().len()
                ),
            )
            .field("send_events", &self.send_events)
            .field(
                "parse",
                &self
                    .parse
                    .as_ref()
                    .map(|_| "Some(StringValueParser)")
                    .unwrap_or("None"),
            )
            .field(
                "marshal",
                &self
                    .marshal
                    .as_ref()
                    .map(|_| "Some(ValueSerializer)")
                    .unwrap_or("None"),
            )
            .finish()
    }
}

impl UpnpVariable for StateVariable {
    fn get_definition(&self) -> &StateVariable {
        return self;
    }
}

impl StateVariable {
    pub fn new(vartype: StateVarType, name: String) -> StateVariable {
        Self {
            object: UpnpObjectType {
                name,
                object_type: "StateVariable".to_string(),
            },
            value_type: vartype.clone(),
            step: None,
            modifiable: true,
            event_conditions: Arc::new(RwLock::new(HashMap::new())),
            description: "".to_string(),
            default_value: None,
            value_range: None,
            allowed_values: Arc::new(RwLock::new(Vec::new())),
            send_events: false,
            parse: None,
            marshal: None,
        }
    }

    pub fn set_step(&mut self, step: StateValue) -> Result<(), StateValueError> {
        if self.as_state_var_type() != step.as_state_var_type() {
            return Err(StateValueError::TypeError("Bad step type".to_string()));
        }

        self.step = Some(step);
        Ok(())
    }

    pub fn set_range(&mut self, min: &StateValue, max: &StateValue) -> Result<(), StateValueError> {
        if self.as_state_var_type() != min.as_state_var_type() {
            return Err(StateValueError::TypeError("Bad range type".to_string()));
        }

        let range = ValueRange::new(min, max)?; // ? propage l'erreur si elle existe
        self.value_range = Some(range);
        Ok(())
    }

    pub fn update_minimum(&mut self, min: &StateValue) -> Result<(), StateValueError> {
        if !self.has_range() {
            return Err(StateValueError::RangeError(
                "No range specified for this variable".to_string(),
            ));
        }
        if self
            .value_range
            .as_ref()
            .expect("Range is not defined")
            .as_state_var_type()
            != min.as_state_var_type()
        {
            return Err(StateValueError::TypeError(
                "new minimum is not the same than state variable".to_string(),
            ));
        }
        self.value_range
            .as_mut()
            .expect("Range is not defined")
            .set_minimum(min);
        return Ok(());
    }

    pub fn update_maximum(&mut self, min: &StateValue) -> Result<(), StateValueError> {
        if !self.has_range() {
            return Err(StateValueError::RangeError(
                "No range specified for this variable".to_string(),
            ));
        }
        if self
            .value_range
            .as_ref()
            .expect("Range is not defined")
            .as_state_var_type()
            != min.as_state_var_type()
        {
            return Err(StateValueError::TypeError(
                "new minimum is not the same than state variable".to_string(),
            ));
        }
        self.value_range
            .as_mut()
            .expect("Range is not defined")
            .set_maximum(min);
        return Ok(());
    }

    pub fn get_range(&self) -> Option<&ValueRange> {
        return self.value_range.as_ref();
    }

    pub fn set_modifiable(&mut self) {
        self.modifiable = true;
    }

    pub fn set_not_modifiable(&mut self) {
        self.modifiable = false;
    }

    pub fn add_event_condition(&self, name: String, func: StateConditionFunc) {
        // on lock en écriture
        let mut guard = self.event_conditions.blocking_write();
        guard.insert(name, func);
        // le lock est automatiquement relâché ici (RAII)
    }

    pub fn remove_event_condition(&self, name: &str) {
        let mut guard = self.event_conditions.blocking_write();
        guard.remove(name);
    }

    pub fn clear_event_conditions(&mut self) {
        let mut guard = self.event_conditions.blocking_write();
        guard.clear()
    }

    pub fn set_description(&mut self, description: String) {
        self.description = description;
    }

    pub fn set_default(&mut self, value: &StateValue) -> Result<(), StateValueError> {
        if self.as_state_var_type() != value.as_state_var_type() {
            return Err(StateValueError::TypeError(
                "value does not have the right type".to_string(),
            ));
        }
        self.default_value = Some(value.clone());
        return Ok(());
    }

    pub fn unset_default(&mut self) {
        self.default_value = None;
    }

    pub fn extend_allowed_values(&mut self, values: &[StateValue]) -> Result<(), StateValueError> {
        let mut av = self
            .allowed_values
            .blocking_write();

        for v in values {
            if self.as_state_var_type() == v.as_state_var_type() {
                av.push(v.clone());
            } else {
                return Err(StateValueError::TypeError(
                    "new allowed value does not have the right type".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub fn push_allowed_value(&mut self, value: &StateValue) -> Result<(), StateValueError> {
        let mut av = self
            .allowed_values
            .blocking_write();

        if self.as_state_var_type() == value.as_state_var_type() {
            av.push(value.clone());
        } else {
            return Err(StateValueError::TypeError(
                "new allowed value does not have the right type".to_string(),
            ));
        }

        return Ok(());
    }

    pub fn set_send_notification(&mut self) {
        self.send_events = true;
    }

    pub fn unset_send_notification(&mut self) {
        self.send_events = false;
    }

    pub fn set_value_parser(&mut self, parser: StringValueParser) -> Result<(), StateValueError> {
        if self.as_state_var_type() == StateVarType::String {
            self.parse = Some(parser);
            return Ok(());
        }
        return Err(StateValueError::TypeError(
            "Only String variables can have a parser".to_string(),
        ));
    }

    pub fn unset_value_parser(&mut self) {
        self.parse = None;
    }

    pub fn set_value_marshaler(
        &mut self,
        marshaler: ValueSerializer,
    ) -> Result<(), StateValueError> {
        if self.as_state_var_type() == StateVarType::String {
            self.marshal = Some(marshaler);
            return Ok(());
        }
        return Err(StateValueError::TypeError(
            "Only String variables can have a marshaler".to_string(),
        ));
    }

    pub fn unset_value_marshaler(&mut self) {
        self.marshal = None;
    }
}
