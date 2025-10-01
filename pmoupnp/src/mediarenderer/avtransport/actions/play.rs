use crate::mediarenderer::avtransport::variables::{A_ARG_TYPE_INSTANCE_ID, TRANSPORTPLAYSPEED};
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