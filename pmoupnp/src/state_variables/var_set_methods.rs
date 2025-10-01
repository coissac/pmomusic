use std::collections::HashMap;

use crate::{state_variables::{StateVariable, StateVariableSet}, UpnpObject};

impl StateVariableSet {
    pub fn new() -> Self {
        Self {
            instances: HashMap::new(),
        }
    }

    pub fn insert(&mut self, instance: StateVariable) {
        self.instances.insert(instance.get_name().clone(), instance);
    }

    pub fn contains(&self, name: &str) -> bool {
        self.instances.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&StateVariable> {
        self.instances.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &StateVariable> {
        self.instances.values()
    }

    pub fn all(&self) -> Vec<&StateVariable> {
        self.instances.values().collect()
    }


}

