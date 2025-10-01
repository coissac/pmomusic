use crate::{
    state_variables::StateVariable,
    variable_types::{StateValue, UpnpVarType},
};

pub trait UpnpVariable {
    fn get_definition(&self) -> &StateVariable;

    fn has_step(&self) -> bool {
        return self.get_definition().step.is_some();
    }

    fn get_step(&self) -> Option<StateValue> {
        return self.get_definition().step.clone();
    }

    fn has_range(&self) -> bool {
        return self.get_definition().value_range.is_some();
    }

    fn is_modifiable(&self) -> bool {
        return self.get_definition().modifiable;
    }

    fn has_event_conditions(&self) -> bool {
        return self.get_definition().event_conditions.read().unwrap().len() > 0;
    }

    fn has_event_condition(&self, name: &String) -> bool {
        let guard = self.get_definition().event_conditions.read().unwrap();
        return guard.contains_key(name);
    }

    fn has_description(&self) -> bool {
        return !String::is_empty(&self.get_definition().description);
    }

    fn get_description(&self) -> String {
        return self.get_definition().description.clone();
    }

    fn has_default(&self) -> bool {
        return self.get_definition().default_value.is_some();
    }

    fn get_default(&self) -> StateValue {
        self.get_definition()
            .default_value
            .clone()
            .unwrap_or_else(|| self.get_definition().as_state_var_type().default_value())
    }

    fn has_allowed_values(&self) -> bool {
        return self.get_definition().allowed_values.read().unwrap().len() > 0;
    }

    fn is_an_allowed_values(&self, value: &StateValue) -> bool {
        let guard = self.get_definition().allowed_values.read().unwrap();
        return guard.contains(value);
    }

    fn is_sending_notification(&self) -> bool {
        self.get_definition().send_events
    }

    fn has_value_parser(&self) -> bool {
        self.get_definition().parse.is_some()
    }

    fn has_value_marshaler(&self) -> bool {
        self.get_definition().marshal.is_some()
    }
}
