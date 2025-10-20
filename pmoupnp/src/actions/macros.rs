/// Macro pour définir facilement une action UPnP.
///
/// Cette macro simplifie la création d'actions UPnP statiques en générant
/// automatiquement le code nécessaire pour initialiser une action avec ses arguments.
///
/// # Syntaxe
///
/// ## Action avec arguments
///
/// ```ignore
/// define_action! {
///     pub static ACTION_NAME = "ActionName" {
///         in "ParamName" => VARIABLE_REF,
///         out "ResultParam" => RESULT_VAR,
///     }
/// }
/// ```
///
/// ## Action sans arguments
///
/// ```ignore
/// define_action! {
///     pub static ACTION_NAME = "ActionName"
/// }
/// ```
///
/// ## Action avec handler personnalisé
///
/// ```ignore
/// define_action! {
///     pub static ACTION_NAME = "ActionName" {
///         in "ParamName" => VARIABLE_REF,
///         out "ResultParam" => RESULT_VAR,
///     }
///     with handler action_handler!(|instance, data| {
///         // Logique personnalisée
///         Ok(())
///     })
/// }
/// ```
///
/// # Arguments
///
/// - `ACTION_NAME` : Nom de la constante statique Rust
/// - `"ActionName"` : Nom de l'action UPnP (chaîne littérale)
/// - `in` ou `out` : Direction de l'argument (entrée ou sortie)
/// - `"ParamName"` : Nom du paramètre UPnP (chaîne littérale)
/// - `VARIABLE_REF` : Référence vers une `Lazy<Arc<StateVariable>>`
///
/// # Type de retour
///
/// La macro génère une `Lazy<Arc<Action>>` qui sera initialisée lors du premier accès.
///
/// # Prérequis
///
/// Les variables d'état référencées doivent être définies comme :
///
/// ```ignore
/// pub static MY_VAR: Lazy<Arc<StateVariable>> = Lazy::new(|| {
///     Arc::new(StateVariable::new(StateVarType::UI4, "MyVar".to_string()))
/// });
/// ```
///
/// # Examples
///
/// ```ignore
/// use once_cell::sync::Lazy;
/// use std::sync::Arc;
///
/// // Définir les variables d'état
/// pub static INSTANCE_ID: Lazy<Arc<StateVariable>> = Lazy::new(|| {
///     Arc::new(StateVariable::new(StateVarType::UI4, "InstanceID".to_string()))
/// });
///
/// pub static TRANSPORT_URI: Lazy<Arc<StateVariable>> = Lazy::new(|| {
///     Arc::new(StateVariable::new(StateVarType::String, "TransportURI".to_string()))
/// });
///
/// // Définir une action avec arguments
/// define_action! {
///     pub static PLAY = "Play" {
///         in "InstanceID" => INSTANCE_ID,
///         in "Speed" => TRANSPORT_SPEED,
///     }
/// }
///
/// // Action sans arguments
/// define_action! {
///     pub static PAUSE = "Pause"
/// }
///
/// // Utilisation
/// fn main() {
///     let play_action = &*PLAY; // Déréférence la Lazy<Arc<Action>>
///     println!("Action: {}", play_action.get_name());
/// }
/// ```
///
/// # Notes d'implémentation
///
/// - Les `Arc<StateVariable>` sont clonés (shallow copy du pointeur)
/// - Chaque `Argument` est wrappé dans un `Arc`
/// - L'`Action` finale est wrappée dans un `Arc`
/// - Initialisation paresseuse via `Lazy` (thread-safe)
#[macro_export]
macro_rules! define_action {
    // Variante stateless avec arguments
    (pub static $name:ident = $action_name:literal stateless {
        $(
            $direction:ident $arg_name:literal => $var_ref:expr
        ),* $(,)?
    }
    $(with handler $handler:expr)?
    ) => {
        pub static $name: once_cell::sync::Lazy<std::sync::Arc<$crate::actions::Action>> =
            once_cell::sync::Lazy::new(|| {
                let mut ac = $crate::actions::Action::new($action_name.to_string());
                ac.set_stateful(false);

                $(
                    ac.add_argument(
                        define_action!(@arg $direction $arg_name, $var_ref)
                    );
                )*

                $(
                    ac.set_handler($handler);
                )?

                std::sync::Arc::new(ac)
            });
    };

    // Variante stateless sans arguments
    (pub static $name:ident = $action_name:literal stateless
    $(with handler $handler:expr)?
    ) => {
        pub static $name: once_cell::sync::Lazy<std::sync::Arc<$crate::actions::Action>> =
            once_cell::sync::Lazy::new(|| {
                let mut ac = $crate::actions::Action::new($action_name.to_string());
                ac.set_stateful(false);

                $(
                    ac.set_handler($handler);
                )?

                std::sync::Arc::new(ac)
            });
    };

    // Variante stateful (défaut) avec arguments
    (pub static $name:ident = $action_name:literal {
        $(
            $direction:ident $arg_name:literal => $var_ref:expr
        ),* $(,)?
    }
    $(with handler $handler:expr)?
    ) => {
        pub static $name: once_cell::sync::Lazy<std::sync::Arc<$crate::actions::Action>> =
            once_cell::sync::Lazy::new(|| {
                let mut ac = $crate::actions::Action::new($action_name.to_string());

                $(
                    ac.add_argument(
                        define_action!(@arg $direction $arg_name, $var_ref)
                    );
                )*

                $(
                    ac.set_handler($handler);
                )?

                std::sync::Arc::new(ac)
            });
    };

    // Variante stateful (défaut) sans arguments
    (pub static $name:ident = $action_name:literal
    $(with handler $handler:expr)?
    ) => {
        pub static $name: once_cell::sync::Lazy<std::sync::Arc<$crate::actions::Action>> =
            once_cell::sync::Lazy::new(|| {
                let mut ac = $crate::actions::Action::new($action_name.to_string());

                $(
                    ac.set_handler($handler);
                )?

                std::sync::Arc::new(ac)
            });
    };

    // Helper interne pour créer un argument d'entrée
    (@arg in $name:literal, $var:expr) => {
        std::sync::Arc::new(
            $crate::actions::Argument::new_in(
                $name.to_string(),
                std::sync::Arc::clone(&$var)
            )
        )
    };

    // Helper interne pour créer un argument de sortie
    (@arg out $name:literal, $var:expr) => {
        std::sync::Arc::new(
            $crate::actions::Argument::new_out(
                $name.to_string(),
                std::sync::Arc::clone(&$var)
            )
        )
    };
}

/// Macro pour définir plusieurs actions UPnP en une seule déclaration.
///
/// Cette macro permet de regrouper la définition de plusieurs actions pour
/// améliorer la lisibilité et réduire la répétition de code.
///
/// # Syntaxe
///
/// ```ignore
/// define_actions! {
///     ACTION1 = "Action1" {
///         in "Param1" => VAR1,
///         out "Result1" => VAR2,
///     }
///     
///     ACTION2 = "Action2" {
///         in "Param1" => VAR1,
///     }
///     
///     ACTION3 = "Action3"
/// }
/// ```
///
/// # Arguments
///
/// Chaque action suit la même syntaxe que [`define_action!`], mais sans
/// le mot-clé `pub static`.
///
/// # Type de retour
///
/// Génère une `Lazy<Arc<Action>>` pour chaque action définie.
///
/// # Examples
///
/// ```ignore
/// use once_cell::sync::Lazy;
/// use std::sync::Arc;
///
/// // Variables d'état
/// pub static INSTANCE_ID: Lazy<Arc<StateVariable>> = Lazy::new(|| {
///     Arc::new(StateVariable::new(StateVarType::UI4, "InstanceID".to_string()))
/// });
///
/// pub static TRANSPORT_URI: Lazy<Arc<StateVariable>> = Lazy::new(|| {
///     Arc::new(StateVariable::new(StateVarType::String, "TransportURI".to_string()))
/// });
///
/// pub static URI_METADATA: Lazy<Arc<StateVariable>> = Lazy::new(|| {
///     Arc::new(StateVariable::new(StateVarType::String, "URIMetaData".to_string()))
/// });
///
/// // Définir plusieurs actions ensemble
/// define_actions! {
///     PLAY = "Play" {
///         in "InstanceID" => INSTANCE_ID,
///     }
///     
///     STOP = "Stop" {
///         in "InstanceID" => INSTANCE_ID,
///     }
///     
///     PAUSE = "Pause" {
///         in "InstanceID" => INSTANCE_ID,
///     }
///     
///     SET_AV_TRANSPORT_URI = "SetAVTransportURI" {
///         in "InstanceID" => INSTANCE_ID,
///         in "CurrentURI" => TRANSPORT_URI,
///         in "CurrentURIMetaData" => URI_METADATA,
///     }
/// }
///
/// // Utilisation
/// fn setup_transport_service() {
///     let actions = vec![&*PLAY, &*STOP, &*PAUSE, &*SET_AV_TRANSPORT_URI];
///     for action in actions {
///         println!("Action: {}", action.get_name());
///     }
/// }
/// ```
///
/// # Avantages
///
/// - Regroupement logique des actions d'un service
/// - Réduction de la répétition de `pub static` et `define_action!`
/// - Meilleure lisibilité pour les services avec nombreuses actions
///
/// # Notes
///
/// - Toutes les actions définies sont publiques (`pub`)
/// - Chaque action est indépendante et peut être utilisée séparément
/// - La macro se développe en plusieurs appels à [`define_action!`]
#[macro_export]
macro_rules! define_actions {
    (
        $(
            $name:ident = $action_name:literal $(stateless)?
            $({
                $(
                    $direction:ident $arg_name:literal => $var_ref:expr
                ),* $(,)?
            })?
        )*
    ) => {
        $(
            define_action! {
                pub static $name = $action_name
                $(stateless)?
                $(
                    {
                        $($direction $arg_name => $var_ref),*
                    }
                )?
            }
        )*
    };
}
