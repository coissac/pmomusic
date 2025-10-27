//! Macros pour générer des extension traits typés sur les métadonnées
//!
//! La macro `define_metadata_properties!` génère automatiquement :
//! - Un trait avec des méthodes `get_XXX()` et `set_XXX()` pour chaque métadonnée
//! - L'implémentation complète pour `Cache<Config>`
//! - Les conversions JSON ↔ Rust selon le type
//!
//! # Types supportés
//!
//! - `String` : Métadonnée texte (JSON String)
//! - `i64` : Métadonnée numérique entière signée (JSON Number)
//! - `f64` : Métadonnée numérique décimale (JSON Number)
//! - `bool` : Métadonnée booléenne (JSON Boolean)
//! - `Value` : Métadonnée JSON brute (Array, Object, ou tout type JSON)
//!
//! # Mécanisme de stockage
//!
//! - Types simples (String, Number, Boolean) : stockés directement
//! - Types complexes (Array, Object) : sérialisés en string JSON, parsés automatiquement à la lecture
//! - La conversion est transparente grâce à `decode_metadata_value`
//!
//! # Exemple
//!
//! ```ignore
//! use pmocache::define_metadata_properties;
//!
//! struct AudioConfig;
//! impl CacheConfig for AudioConfig { ... }
//!
//! define_metadata_properties! {
//!     AudioMetadataExt for pmocache::Cache<AudioConfig> {
//!         title: String as string,
//!         duration_secs: i64 as i64,
//!         custom_tags: serde_json::Value as value,  // Pour JSON complexe
//!     }
//! }
//!
//! // Utilisation
//! use AudioMetadataExt;
//! let title = cache.get_title("pk123").await?;
//! cache.set_title("pk123", "New Title".into()).await?;
//!
//! // JSON complexe
//! let tags = json!({"mood": "happy", "bpm": 120});
//! cache.set_custom_tags("pk123", tags).await?;
//! ```

/// Génère un extension trait pour accéder aux métadonnées de manière typée
///
/// Cette macro génère un trait complet avec toutes les méthodes get/set
/// et son implémentation pour le type de cache spécifié.
///
/// # Syntaxe
///
/// ```ignore
/// define_metadata_properties! {
///     TraitName for CacheType {
///         field_name: RustType as type_kind,
///         field_name2: RustType as type_kind,
///         ...
///     }
/// }
/// ```
///
/// Type kinds available: `string`, `i64`, `f64`, `bool`, `value`
///
/// Pour chaque champ, génère :
/// - `async fn get_FIELD(&self, pk: &str) -> Result<Option<TYPE>>`
/// - `async fn set_FIELD(&self, pk: &str, value: TYPE) -> Result<()>`
///
/// La clé JSON utilisée est le nom du champ (ex: `title` → clé `"title"`).
#[macro_export]
macro_rules! define_metadata_properties {
    (
        $trait_name:ident for $cache_type:ty {
            $(
                $field:ident: $rust_type:ty as $type_kind:ident
            ),* $(,)?
        }
    ) => {
        // Définition du trait
        pub trait $trait_name {
            $(
                // Génère get_FIELD
                paste::paste! {
                    async fn [<get_ $field>](&self, pk: &str) -> anyhow::Result<Option<$rust_type>>;
                }

                // Génère set_FIELD
                paste::paste! {
                    async fn [<set_ $field>](&self, pk: &str, value: $rust_type) -> anyhow::Result<()>;
                }
            )*
        }

        // Implémentation du trait
        impl $trait_name for $cache_type {
            $(
                // Implémentation de get_FIELD selon le type
                $crate::__impl_getter!($field, $rust_type, $type_kind);

                // Implémentation de set_FIELD selon le type
                $crate::__impl_setter!($field, $rust_type, $type_kind);
            )*
        }
    };
}

// ============================================================================
// Macros internes pour générer les getters selon le type
// ============================================================================

#[doc(hidden)]
#[macro_export]
macro_rules! __impl_getter {
    // String - utilise get_a_metadata_as_string
    ($field:ident, $rust_type:ty, string) => {
        paste::paste! {
            async fn [<get_ $field>](&self, pk: &str) -> anyhow::Result<Option<$rust_type>> {
                self.get_a_metadata_as_string(pk, stringify!($field)).await
            }
        }
    };

    // i64 - utilise get_a_metadata_as_number puis as_i64()
    ($field:ident, $rust_type:ty, i64) => {
        paste::paste! {
            async fn [<get_ $field>](&self, pk: &str) -> anyhow::Result<Option<$rust_type>> {
                match self.get_a_metadata_as_number(pk, stringify!($field)).await? {
                    Some(n) => Ok(n.as_i64()),
                    None => Ok(None),
                }
            }
        }
    };

    // f64 - utilise get_a_metadata_as_number puis as_f64()
    ($field:ident, $rust_type:ty, f64) => {
        paste::paste! {
            async fn [<get_ $field>](&self, pk: &str) -> anyhow::Result<Option<$rust_type>> {
                match self.get_a_metadata_as_number(pk, stringify!($field)).await? {
                    Some(n) => Ok(n.as_f64()),
                    None => Ok(None),
                }
            }
        }
    };

    // bool - utilise get_a_metadata_as_bool
    ($field:ident, $rust_type:ty, bool) => {
        paste::paste! {
            async fn [<get_ $field>](&self, pk: &str) -> anyhow::Result<Option<$rust_type>> {
                self.get_a_metadata_as_bool(pk, stringify!($field)).await
            }
        }
    };

    // Value - utilise get_a_metadata directement (retourne JSON brut)
    ($field:ident, $rust_type:ty, value) => {
        paste::paste! {
            async fn [<get_ $field>](&self, pk: &str) -> anyhow::Result<Option<$rust_type>> {
                self.get_a_metadata(pk, stringify!($field)).await
            }
        }
    };
}

// ============================================================================
// Macros internes pour générer les setters selon le type
// ============================================================================

#[doc(hidden)]
#[macro_export]
macro_rules! __impl_setter {
    // String - stocke comme Value::String
    ($field:ident, $rust_type:ty, string) => {
        paste::paste! {
            async fn [<set_ $field>](&self, pk: &str, value: $rust_type) -> anyhow::Result<()> {
                use serde_json::Value;
                self.db.set_a_metadata(pk, stringify!($field), Value::String(value))
                    .map_err(|e| anyhow::anyhow!("DB error: {}", e))
            }
        }
    };

    // i64 - stocke comme Value::Number
    ($field:ident, $rust_type:ty, i64) => {
        paste::paste! {
            async fn [<set_ $field>](&self, pk: &str, value: $rust_type) -> anyhow::Result<()> {
                use serde_json::{Value, Number};
                self.db.set_a_metadata(
                    pk,
                    stringify!($field),
                    Value::Number(Number::from(value))
                )
                .map_err(|e| anyhow::anyhow!("DB error: {}", e))
            }
        }
    };

    // f64 - stocke comme Value::Number (avec validation)
    ($field:ident, $rust_type:ty, f64) => {
        paste::paste! {
            async fn [<set_ $field>](&self, pk: &str, value: $rust_type) -> anyhow::Result<()> {
                use serde_json::{Value, Number};
                let number = Number::from_f64(value)
                    .ok_or_else(|| anyhow::anyhow!("Invalid f64 value: {}", value))?;
                self.db.set_a_metadata(pk, stringify!($field), Value::Number(number))
                    .map_err(|e| anyhow::anyhow!("DB error: {}", e))
            }
        }
    };

    // bool - stocke comme Value::Bool
    ($field:ident, $rust_type:ty, bool) => {
        paste::paste! {
            async fn [<set_ $field>](&self, pk: &str, value: $rust_type) -> anyhow::Result<()> {
                use serde_json::Value;
                self.db.set_a_metadata(pk, stringify!($field), Value::Bool(value))
                    .map_err(|e| anyhow::anyhow!("DB error: {}", e))
            }
        }
    };

    // Value - stocke directement (Array/Object sont sérialisés automatiquement)
    ($field:ident, $rust_type:ty, value) => {
        paste::paste! {
            async fn [<set_ $field>](&self, pk: &str, value: $rust_type) -> anyhow::Result<()> {
                self.db.set_a_metadata(pk, stringify!($field), value)
                    .map_err(|e| anyhow::anyhow!("DB error: {}", e))
            }
        }
    };
}
