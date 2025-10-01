mod errors;

mod action_methods;
mod action_instance;
mod action_set_methods;
mod action_instance_set;
mod argument;
mod arg_set_methods;


use std::collections::HashMap;
use crate::{state_variables::StateVariable, UpnpObjectType};


#[derive(Debug, Clone)]
struct Action {
    object: UpnpObjectType,
    arguments: ArgumentSet,
}

#[derive(Debug, Default, Clone)]
pub struct ActionSet {
    actions: HashMap<String, Action>,
}

#[derive(Debug, Clone)]
pub struct ActionInstance {
    object: UpnpObjectType,
    model: Action,
}

#[derive(Debug, Default, Clone)]
pub struct ActionInstanceSet {
    instances: HashMap<String, ActionInstance>,
}

#[derive(Debug, Clone)]
pub struct Argument {
    object: UpnpObjectType,
    state_variable: StateVariable,
    is_in: bool,
    is_out: bool,
}

#[derive(Debug, Default, Clone)]
pub struct ArgumentSet {
    arguments: HashMap<String, Argument>,
}
