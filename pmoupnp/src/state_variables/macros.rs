/// Macro pour définir facilement une variable d'état UPnP.
///
/// Cette macro simplifie la création de variables d'état UPnP statiques en générant
/// automatiquement le code nécessaire pour initialiser une variable avec ses propriétés.
///
/// # Syntaxe
///
/// ## Variable simple (sans valeurs autorisées)
///
/// ```ignore
/// define_variable! {
///     pub static VAR_NAME: Type = "VariableName"
/// }
/// ```
///
/// ## Variable avec valeurs autorisées (enum)
///
/// ```ignore
/// define_variable! {
///     pub static VAR_NAME: String = "VariableName" {
///         allowed: ["Value1", "Value2", "Value3"],
///     }
/// }
/// ```
///
/// ## Variable avec valeur par défaut
///
/// ```ignore
/// define_variable! {
///     pub static VAR_NAME: String = "VariableName" {
///         allowed: ["Value1", "Value2"],
///         default: "Value1",
///     }
/// }
/// ```
///
/// ## Variable avec notification d'événements
///
/// ```ignore
/// define_variable! {
///     pub static VAR_NAME: String = "VariableName" {
///         allowed: ["STOPPED", "PLAYING"],
///         evented: true,
///     }
/// }
/// ```
///
/// ## Variable avec toutes les options
///
/// ```ignore
/// define_variable! {
///     pub static VAR_NAME: String = "VariableName" {
///         allowed: ["Value1", "Value2", "Value3"],
///         default: "Value1",
///         evented: true,
///     }
/// }
/// ```
///
/// # Types supportés
///
/// - `String` : Chaîne de caractères
/// - `UI1`, `UI2`, `UI4` : Entiers non signés
/// - `I1`, `I2`, `I4` : Entiers signés
/// - `Boolean` : Booléen
///
/// # Arguments optionnels
///
/// - `allowed: [...]` : Liste des valeurs autorisées (enum)
/// - `default: "..."` : Valeur par défaut
/// - `evented: true` : Active les notifications d'événements
///
/// # Examples
///
/// ```ignore
/// use pmoupnp::define_variable;
///
/// // Variable simple
/// define_variable! {
///     pub static VOLUME: UI2 = "Volume"
/// }
///
/// // Variable avec enum
/// define_variable! {
///     pub static MUTE: String = "Mute" {
///         allowed: ["0", "1"],
///         default: "0",
///     }
/// }
///
/// // Variable avec notification
/// define_variable! {
///     pub static TRANSPORT_STATE: String = "TransportState" {
///         allowed: ["STOPPED", "PLAYING", "PAUSED_PLAYBACK"],
///         evented: true,
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_variable {
    // Variable simple sans options
    (pub static $name:ident: $type:ident = $var_name:literal) => {
        pub static $name: once_cell::sync::Lazy<std::sync::Arc<$crate::state_variables::StateVariable>> =
            once_cell::sync::Lazy::new(|| {
                let sv = $crate::state_variables::StateVariable::new(
                    $crate::variable_types::StateVarType::$type,
                    $var_name.to_string()
                );
                std::sync::Arc::new(sv)
            });
    };

    // Variable avec options
    (pub static $name:ident: $type:ident = $var_name:literal {
        $(allowed: [$($value:literal),* $(,)?],)?
        $(default: $default:literal,)?
        $(evented: $evented:literal,)?
    }) => {
        pub static $name: once_cell::sync::Lazy<std::sync::Arc<$crate::state_variables::StateVariable>> =
            once_cell::sync::Lazy::new(|| {
                let mut sv = $crate::state_variables::StateVariable::new(
                    $crate::variable_types::StateVarType::$type,
                    $var_name.to_string()
                );

                $(
                    sv.extend_allowed_values(&[
                        $(define_variable!(@value $type, $value)),*
                    ]).expect(&format!("Cannot set allowed values for {}", $var_name));
                )?

                $(
                    sv.set_default(&define_variable!(@value $type, $default))
                        .expect(&format!("Cannot set default value for {}", $var_name));
                )?

                $(
                    if $evented {
                        sv.set_send_notification();
                    }
                )?

                std::sync::Arc::new(sv)
            });
    };

    // Helper pour créer une StateValue selon le type
    (@value String, $val:literal) => {
        $crate::variable_types::StateValue::String($val.to_string())
    };
    (@value UI1, $val:literal) => {
        $crate::variable_types::StateValue::UI1($val)
    };
    (@value UI2, $val:literal) => {
        $crate::variable_types::StateValue::UI2($val)
    };
    (@value UI4, $val:literal) => {
        $crate::variable_types::StateValue::UI4($val)
    };
    (@value I1, $val:literal) => {
        $crate::variable_types::StateValue::I1($val)
    };
    (@value I2, $val:literal) => {
        $crate::variable_types::StateValue::I2($val)
    };
    (@value I4, $val:literal) => {
        $crate::variable_types::StateValue::I4($val)
    };
    (@value Boolean, $val:literal) => {
        $crate::variable_types::StateValue::Boolean($val)
    };
}
