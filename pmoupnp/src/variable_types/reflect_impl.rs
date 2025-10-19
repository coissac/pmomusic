// Ce module permet de convertir StateValue en valeurs Reflect et vice-versa
//
// Étant donné que StateValue contient des types qui n'implémentent pas tous Reflect
// (comme Uuid, Url, et certains types chrono), nous fournissons des méthodes de conversion
// vers des types primitifs qui supportent Reflect.

use crate::variable_types::{StateValue, StateValueError, StateVarType};
use bevy_reflect::Reflect;

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

    /// Convertit &dyn Reflect → StateValue selon le type attendu
    ///
    /// Méthode statique utilisée pour reconstruire StateValue depuis Reflect
    pub fn from_reflect(
        value: &dyn Reflect,
        expected_type: StateVarType,
    ) -> Result<StateValue, StateValueError> {
        match expected_type {
            StateVarType::UI1 => value
                .as_any()
                .downcast_ref::<u8>()
                .map(|v| StateValue::UI1(*v))
                .ok_or_else(|| StateValueError::TypeError("Expected u8".into())),
            StateVarType::UI2 => value
                .as_any()
                .downcast_ref::<u16>()
                .map(|v| StateValue::UI2(*v))
                .ok_or_else(|| StateValueError::TypeError("Expected u16".into())),
            StateVarType::UI4 => value
                .as_any()
                .downcast_ref::<u32>()
                .map(|v| StateValue::UI4(*v))
                .ok_or_else(|| StateValueError::TypeError("Expected u32".into())),
            StateVarType::I1 => value
                .as_any()
                .downcast_ref::<i8>()
                .map(|v| StateValue::I1(*v))
                .ok_or_else(|| StateValueError::TypeError("Expected i8".into())),
            StateVarType::I2 => value
                .as_any()
                .downcast_ref::<i16>()
                .map(|v| StateValue::I2(*v))
                .ok_or_else(|| StateValueError::TypeError("Expected i16".into())),
            StateVarType::I4 | StateVarType::Int => value
                .as_any()
                .downcast_ref::<i32>()
                .map(|v| StateValue::I4(*v))
                .ok_or_else(|| StateValueError::TypeError("Expected i32".into())),
            StateVarType::R4 => value
                .as_any()
                .downcast_ref::<f32>()
                .map(|v| StateValue::R4(*v))
                .ok_or_else(|| StateValueError::TypeError("Expected f32".into())),
            StateVarType::R8 | StateVarType::Number | StateVarType::Fixed14_4 => value
                .as_any()
                .downcast_ref::<f64>()
                .map(|v| StateValue::R8(*v))
                .ok_or_else(|| StateValueError::TypeError("Expected f64".into())),
            StateVarType::String | StateVarType::BinBase64 | StateVarType::BinHex => value
                .as_any()
                .downcast_ref::<String>()
                .map(|v| match expected_type {
                    StateVarType::String => StateValue::String(v.clone()),
                    StateVarType::BinBase64 => StateValue::BinBase64(v.clone()),
                    StateVarType::BinHex => StateValue::BinHex(v.clone()),
                    _ => unreachable!(),
                })
                .ok_or_else(|| StateValueError::TypeError("Expected String".into())),
            StateVarType::Boolean => value
                .as_any()
                .downcast_ref::<bool>()
                .map(|v| StateValue::Boolean(*v))
                .ok_or_else(|| StateValueError::TypeError("Expected bool".into())),
            StateVarType::Char => value
                .as_any()
                .downcast_ref::<char>()
                .map(|v| StateValue::Char(*v))
                .ok_or_else(|| StateValueError::TypeError("Expected char".into())),
            // Pour les types complexes, on essaie de reconstruire depuis String
            StateVarType::Date
            | StateVarType::DateTime
            | StateVarType::DateTimeTZ
            | StateVarType::Time
            | StateVarType::TimeTZ
            | StateVarType::UUID
            | StateVarType::URI => {
                value
                    .as_any()
                    .downcast_ref::<String>()
                    .ok_or_else(|| {
                        StateValueError::TypeError("Expected String representation".into())
                    })
                    .and_then(|s| {
                        // Utiliser les méthodes from_string existantes
                        StateValue::from_string(s, &expected_type)
                    })
            }
        }
    }
}
