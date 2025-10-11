// Ce module permet de convertir StateValue en valeurs Reflect
//
// Étant donné que StateValue contient des types qui n'implémentent pas tous Reflect
// (comme Uuid, Url, et certains types chrono), nous fournissons des méthodes de conversion
// vers des types primitifs qui supportent Reflect.

use bevy_reflect::Reflect;
use crate::variable_types::StateValue;

impl StateValue {
    /// Convertit la StateValue en une valeur Reflect.
    ///
    /// Cette méthode crée un Box<dyn Reflect> contenant la valeur sous-jacente.
    /// Pour les types qui n'implémentent pas Reflect nativement (Uuid, Url, dates complexes),
    /// on retourne leur représentation String.
    pub fn to_reflect(&self) -> Box<dyn Reflect> {
        match self {
            StateValue::UI1(v) => Box::new(*v),
            StateValue::UI2(v) => Box::new(*v),
            StateValue::UI4(v) => Box::new(*v),
            StateValue::I1(v) => Box::new(*v),
            StateValue::I2(v) => Box::new(*v),
            StateValue::I4(v) => Box::new(*v),
            StateValue::Int(v) => Box::new(*v),
            StateValue::R4(v) => Box::new(*v),
            StateValue::R8(v) => Box::new(*v),
            StateValue::Number(v) => Box::new(*v),
            StateValue::Fixed14_4(v) => Box::new(*v),
            StateValue::Char(v) => Box::new(*v),
            StateValue::String(v) => Box::new(v.clone()),
            StateValue::Boolean(v) => Box::new(*v),
            StateValue::BinBase64(v) => Box::new(v.clone()),
            StateValue::BinHex(v) => Box::new(v.clone()),
            // Pour les types complexes, on utilise leur représentation String
            StateValue::Date(v) => Box::new(v.to_string()),
            StateValue::DateTime(v) => Box::new(v.to_string()),
            StateValue::DateTimeTZ(v) => Box::new(v.to_string()),
            StateValue::Time(v) => Box::new(v.to_string()),
            StateValue::TimeTZ(v) => Box::new(v.to_string()),
            StateValue::UUID(v) => Box::new(v.to_string()),
            StateValue::URI(v) => Box::new(v.to_string()),
        }
    }
}
