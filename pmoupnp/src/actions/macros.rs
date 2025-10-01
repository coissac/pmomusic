/// Macro pour définir facilement des actions UPnP
///
/// # Syntaxe
///
/// ```ignore
/// define_action! {
///     pub static ACTION_NAME = "ActionName" {
///         in "ParamName" => VARIABLE_REF,
///         in "OtherParam" => OTHER_VAR,
///         out "ResultParam" => RESULT_VAR,
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_action {
    // Variante sans arguments
    (pub static $name:ident = $action_name:literal) => {
        pub static $name: once_cell::sync::Lazy<$crate::actions::Action> = 
            once_cell::sync::Lazy::new(|| {
                $crate::actions::Action::new($action_name.to_string())
            });
    };
    
    // Variante avec arguments
    (pub static $name:ident = $action_name:literal {
        $(
            $direction:ident $arg_name:literal => $var_ref:expr
        ),* $(,)?
    }) => {
        pub static $name: once_cell::sync::Lazy<$crate::actions::Action> = 
            once_cell::sync::Lazy::new(|| {
                let mut ac = $crate::actions::Action::new($action_name.to_string());
                
                $(
                    ac.add_argument(
                        define_action!(@arg $direction $arg_name, $var_ref)
                    );
                )*
                
                ac
            });
    };
    
    // Helpers internes pour créer les arguments
    (@arg in $name:literal, $var:expr) => {
        $crate::actions::Argument::new_in($name.to_string(), $var.clone())
    };
    
    (@arg out $name:literal, $var:expr) => {
        $crate::actions::Argument::new_out($name.to_string(), $var.clone())
    };
}

// ============= Exemples d'utilisation =============

#[cfg(test)]
mod examples {
    use super::*;
    
    // Utilisation originale (pour comparaison)
    mod original {
        use crate::mediarenderer::avtransport::variables::{
            A_ARG_TYPE_INSTANCE_ID, 
            TRANSPORTPLAYSPEED
        };
        use crate::actions::{Action, Argument};
        use once_cell::sync::Lazy;

        pub static PLAY: Lazy<Action> = Lazy::new(|| -> Action {
            let mut ac = Action::new("Play".to_string());

            ac.add_argument(
                Argument::new_in("InstanceID".to_string(), 
                A_ARG_TYPE_INSTANCE_ID.clone())
            );

            ac.add_argument(
                Argument::new_in("Speed".to_string(), 
                TRANSPORTPLAYSPEED.clone())
            );

            ac
        });
    }
    
    // Avec la macro - Version simple
    mod with_macro {
        use crate::mediarenderer::avtransport::variables::*;
        
        define_action! {
            pub static PLAY = "Play" {
                in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
                in "Speed" => TRANSPORTPLAYSPEED,
            }
        }
        
        define_action! {
            pub static SETAVTRANSPORTURI = "SetAVTransportURI" {
                in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
                in "CurrentURI" => AVTRANSPORTURI,
                in "CurrentURIMetaData" => AVTRANSPORTURIMETADATA,
            }
        }
        
        define_action! {
            pub static STOP = "Stop" {
                in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
            }
        }
        
        // Action sans arguments
        define_action! {
            pub static PAUSE = "Pause"
        }
        
    }
}

// ============= Macro alternative encore plus concise =============

/// Macro pour définir plusieurs actions en une fois
///
/// # Syntaxe
///
/// ```ignore
/// define_actions! {
///     PLAY = "Play" {
///         in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
///         in "Speed" => TRANSPORTPLAYSPEED,
///     }
///     
///     STOP = "Stop" {
///         in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_actions {
    (
        $(
            $name:ident = $action_name:literal {
                $(
                    $direction:ident $arg_name:literal => $var_ref:expr
                ),* $(,)?
            }
        )*
    ) => {
        $(
            define_action! {
                pub static $name = $action_name {
                    $($direction $arg_name => $var_ref),*
                }
            }
        )*
    };
    
    // Support pour actions sans arguments
    (
        $(
            $name:ident = $action_name:literal
        )*
    ) => {
        $(
            define_action! {
                pub static $name = $action_name
            }
        )*
    };
}

// ============= Exemple d'utilisation groupée =============

#[cfg(test)]
mod grouped_example {
    use crate::mediarenderer::avtransport::variables::*;
    
    define_actions! {
        PLAY = "Play" {
            in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
            in "Speed" => TRANSPORTPLAYSPEED,
        }
        
        STOP = "Stop" {
            in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
        }
        
        PAUSE = "Pause" {
            in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
        }
        
        SETAVTRANSPORTURI = "SetAVTransportURI" {
            in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
            in "CurrentURI" => AVTRANSPORTURI,
            in "CurrentURIMetaData" => AVTRANSPORTURIMETADATA,
        }
    }
}