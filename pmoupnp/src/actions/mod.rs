mod errors;

mod action_methods;
mod action_instance;
mod action_set_methods;
mod action_instance_set;
mod argument_methods;
mod arg_set_methods;
mod arg_inst_set_methods;
mod arg_instance_methods;

mod macros;


use std::sync::Arc;
use crate::{state_variables::{StateVarInstance, StateVariable}, UpnpObjectSet, UpnpObjectType};

pub use errors::ActionError;

#[derive(Debug, Clone)]
pub struct Action {
    object: UpnpObjectType,
    arguments: ArgumentSet,
}

pub type ActionSet = UpnpObjectSet<Action>;

#[derive(Debug, Clone)]
pub struct ActionInstance {
    object: UpnpObjectType,
    model: Action,
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


#[derive(Debug, Clone)]
pub struct ArgumentInstance {
    object: UpnpObjectType,
    model: Argument,
    variable_instance: Option<Arc<StateVarInstance>>,
}

pub type ArgInstanceSet = UpnpObjectSet<ArgumentInstance>;
