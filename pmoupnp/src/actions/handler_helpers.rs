//! Helpers et macros pour faciliter l'écriture de handlers d'actions.
//!
//! Ce module fournit des fonctions utilitaires et des macros pour simplifier
//! la manipulation de [`ActionData`](crate::actions::ActionData) dans les handlers.
//!
//! # Fonctions utilitaires
//!
//! - [`get_value`] : Extrait une valeur typée depuis ActionData
//! - [`set_value`] : Insère une valeur dans ActionData
//!
//! # Macros
//!
//! - [`get!`](crate::get) : Macro pour extraire facilement une valeur
//! - [`set!`](crate::set) : Macro pour insérer facilement une valeur
//!
//! # Examples
//!
//! ```rust
//! use pmoupnp::{action_handler, get, set};
//! use pmoupnp::actions::ActionError;
//!
//! let handler = action_handler!(|mut data| {
//!     // Extraction avec macro
//!     let celsius: f64 = get!(data, "Celsius", f64);
//!
//!     // Calcul
//!     let fahrenheit = celsius * 9.0 / 5.0 + 32.0;
//!
//!     // Insertion avec macro
//!     set!(data, "Fahrenheit", fahrenheit);
//!
//!     Ok(data)
//! });
//! ```

use crate::actions::{ActionData, ActionError};
use bevy_reflect::Reflect;

/// Extrait une valeur typée depuis ActionData.
///
/// Cette fonction permet d'extraire une valeur `Box<dyn Reflect>` depuis
/// ActionData et de la convertir vers le type concret attendu.
///
/// # Type Parameters
///
/// * `T` - Le type concret attendu (doit implémenter `Reflect + Clone`)
///
/// # Arguments
///
/// * `data` - Référence vers ActionData
/// * `key` - Clé de la valeur à extraire
///
/// # Returns
///
/// `Ok(T)` si la valeur existe et peut être convertie vers `T`,
/// `Err(ActionError)` sinon.
///
/// # Errors
///
/// Retourne `ActionError::ArgumentError` si :
/// - La clé n'existe pas dans ActionData
/// - La valeur ne peut pas être convertie vers le type `T`
///
/// # Examples
///
/// ```rust
/// use pmoupnp::actions::{ActionData, get_value};
/// use std::collections::HashMap;
///
/// let mut data: ActionData = HashMap::new();
/// data.insert("Volume".to_string(), Box::new(50u32));
///
/// let volume: u32 = get_value(&data, "Volume").unwrap();
/// assert_eq!(volume, 50);
/// ```
pub fn get_value<T: Reflect + Clone>(data: &ActionData, key: &str) -> Result<T, ActionError> {
    data.get(key)
        .and_then(|boxed| boxed.as_any().downcast_ref::<T>())
        .cloned()
        .ok_or_else(|| {
            ActionError::ArgumentError(format!("Argument '{}' not found or type mismatch", key))
        })
}

/// Insère une valeur dans ActionData.
///
/// Cette fonction convertit automatiquement la valeur en `Box<dyn Reflect>`
/// et l'insère dans ActionData.
///
/// # Type Parameters
///
/// * `T` - Le type de la valeur (doit implémenter `Reflect + 'static`)
///
/// # Arguments
///
/// * `data` - Référence mutable vers ActionData
/// * `key` - Clé pour la valeur (convertie en `String`)
/// * `value` - Valeur à insérer
///
/// # Examples
///
/// ```rust
/// use pmoupnp::actions::{ActionData, set_value};
/// use std::collections::HashMap;
///
/// let mut data: ActionData = HashMap::new();
/// set_value(&mut data, "Volume", 75u32);
///
/// // Vérifier l'insertion
/// use pmoupnp::actions::get_value;
/// let volume: u32 = get_value(&data, "Volume").unwrap();
/// assert_eq!(volume, 75);
/// ```
pub fn set_value<T: Reflect + 'static>(data: &mut ActionData, key: impl Into<String>, value: T) {
    data.insert(key.into(), Box::new(value));
}

/// Convertit une valeur Reflect en chaîne lisible pour les logs/SOAP.
///
/// Cette fonction réalise une tentative de conversion vers les types
/// primitifs les plus courants (String, entiers, flottants, bool).
/// Si aucune correspondance n'est trouvée, elle utilise `ReflectRef`
/// pour fournir une représentation `Debug` générique.
pub fn reflect_to_string(value: &dyn Reflect) -> String {
    if let Some(v) = value.as_any().downcast_ref::<String>() {
        v.clone()
    } else if let Some(v) = value.as_any().downcast_ref::<&str>() {
        v.to_string()
    } else if let Some(v) = value.as_any().downcast_ref::<u8>() {
        v.to_string()
    } else if let Some(v) = value.as_any().downcast_ref::<u16>() {
        v.to_string()
    } else if let Some(v) = value.as_any().downcast_ref::<u32>() {
        v.to_string()
    } else if let Some(v) = value.as_any().downcast_ref::<u64>() {
        v.to_string()
    } else if let Some(v) = value.as_any().downcast_ref::<i8>() {
        v.to_string()
    } else if let Some(v) = value.as_any().downcast_ref::<i16>() {
        v.to_string()
    } else if let Some(v) = value.as_any().downcast_ref::<i32>() {
        v.to_string()
    } else if let Some(v) = value.as_any().downcast_ref::<i64>() {
        v.to_string()
    } else if let Some(v) = value.as_any().downcast_ref::<f32>() {
        v.to_string()
    } else if let Some(v) = value.as_any().downcast_ref::<f64>() {
        v.to_string()
    } else if let Some(v) = value.as_any().downcast_ref::<bool>() {
        if *v { "1".to_string() } else { "0".to_string() }
    } else {
        "<unsupported Reflect>".to_string()
    }
}

/// Macro pour extraire facilement une valeur depuis ActionData.
///
/// Cette macro simplifie l'utilisation de [`get_value`] en gérant
/// automatiquement la propagation d'erreur avec `?`.
///
/// # Syntaxe
///
/// ```ignore
/// get!(data, "key", Type)
/// ```
///
/// # Arguments
///
/// * `data` - Expression évaluant à `&ActionData`
/// * `key` - Clé de la valeur (expression évaluant à `&str`)
/// * `type` - Type concret attendu
///
/// # Returns
///
/// La valeur de type `Type` si elle existe et peut être convertie,
/// sinon propage l'erreur avec `?`.
///
/// # Examples
///
/// ```ignore
/// use pmoupnp::{get, action_handler};
/// use pmoupnp::actions::ActionError;
///
/// let handler = action_handler!(|data| {
///     let volume: u32 = get!(data, "Volume", u32);
///     let name: String = get!(data, "Name", String);
///
///     // Utiliser les valeurs...
///
///     Ok(data)
/// });
/// ```
#[macro_export]
macro_rules! get {
    ($data:expr, $key:expr, $type:ty) => {
        $crate::actions::get_value::<$type>($data, $key)?
    };
}

/// Macro pour insérer facilement une valeur dans ActionData.
///
/// Cette macro simplifie l'utilisation de [`set_value`] pour
/// insérer des valeurs dans ActionData.
///
/// # Syntaxe
///
/// ```ignore
/// set!(data, "key", value)
/// ```
///
/// # Arguments
///
/// * `data` - Expression évaluant à `&mut ActionData`
/// * `key` - Clé pour la valeur (expression évaluant vers `String`)
/// * `value` - Valeur à insérer (doit implémenter `Reflect + 'static`)
///
/// # Examples
///
/// ```ignore
/// use pmoupnp::{set, action_handler};
///
/// let handler = action_handler!(|mut data| {
///     set!(data, "Result", 42u32);
///     set!(data, "Message", "Success".to_string());
///
///     Ok(data)
/// });
/// ```
#[macro_export]
macro_rules! set {
    ($data:expr, $key:expr, $value:expr) => {
        $crate::actions::set_value($data, $key, $value)
    };
}
