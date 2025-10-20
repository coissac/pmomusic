use std::sync::Arc;

use xmltree::{Element, XMLNode};

use crate::{
    UpnpModel, UpnpObject, UpnpObjectType, UpnpTyped,
    actions::{Argument, ArgumentInstance},
    state_variables::StateVariable,
};

impl UpnpTyped for Argument {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        &self.object
    }
}

impl UpnpObject for Argument {
    fn to_xml_element(&self) -> Element {
        let mut parent = Element::new("argumentList");

        if self.is_in() && self.is_out() {
            // InOut → deux arguments
            parent.children.push(XMLNode::Element(make_argument_elem(
                self.get_name(),
                "in",
                self.state_variable().get_name(),
            )));
            parent.children.push(XMLNode::Element(make_argument_elem(
                self.get_name(),
                "out",
                self.state_variable().get_name(),
            )));
        } else {
            // Cas simple
            let direction = if self.is_in() { "in" } else { "out" };
            parent.children.push(XMLNode::Element(make_argument_elem(
                self.get_name(),
                direction,
                self.state_variable().get_name(),
            )));
        }

        parent
    }
}

impl UpnpModel for Argument {
    type Instance = ArgumentInstance;
}

impl Argument {
    fn new(name: String, state_variable: Arc<StateVariable>) -> Self {
        Self {
            object: UpnpObjectType {
                name,
                object_type: "Argument".to_string(),
            },
            state_variable,
            is_in: false,
            is_out: false,
        }
    }

    pub fn new_in(name: String, state_variable: Arc<StateVariable>) -> Self {
        let mut arg = Self::new(name, state_variable);
        arg.is_in = true;
        arg
    }

    pub fn new_out(name: String, state_variable: Arc<StateVariable>) -> Self {
        let mut arg = Self::new(name, state_variable);
        arg.is_out = true;
        arg
    }

    pub fn new_in_out(name: String, state_variable: Arc<StateVariable>) -> Self {
        let mut arg = Self::new(name, state_variable);
        arg.is_in = true;
        arg.is_out = true;
        arg
    }

    pub fn state_variable(&self) -> &StateVariable {
        &self.state_variable
    }

    pub fn is_in(&self) -> bool {
        self.is_in
    }

    pub fn is_out(&self) -> bool {
        self.is_out
    }
}

/// Fabrique un <argument> complet avec ses sous-éléments
fn make_argument_elem(name: &str, direction: &str, state_var_name: &str) -> Element {
    let mut arg = Element::new("argument");

    let mut name_elem = Element::new("name");
    name_elem.children.push(XMLNode::Text(name.to_string()));

    let mut dir_elem = Element::new("direction");
    dir_elem.children.push(XMLNode::Text(direction.to_string()));

    let mut rel_elem = Element::new("relatedStateVariable");
    rel_elem
        .children
        .push(XMLNode::Text(state_var_name.to_string()));

    arg.children.push(XMLNode::Element(name_elem));
    arg.children.push(XMLNode::Element(dir_elem));
    arg.children.push(XMLNode::Element(rel_elem));

    arg
}
