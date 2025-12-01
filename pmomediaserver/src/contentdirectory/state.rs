use once_cell::sync::OnceCell;
use pmoupnp::{services::ServiceInstance, state_variables::StateVarInstance, variable_types::StateValue};
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicU32, Ordering},
};
use tokio::task;

static CONTENTDIR_INSTANCE: OnceCell<Weak<ServiceInstance>> = OnceCell::new();
static SYSTEM_UPDATE_ID: AtomicU32 = AtomicU32::new(1);
static CONTAINER_UPDATE_IDS: Mutex<String> = Mutex::new(String::new());

/// Enregistre l'instance ContentDirectory pour pouvoir pousser des notifications GENA.
pub fn register_instance(instance: &Arc<ServiceInstance>) {
    let _ = CONTENTDIR_INSTANCE.set(Arc::downgrade(instance));
    // Initialiser les valeurs
    set_system_update_id(1);
    set_container_update_ids("");
}

/// Notifie une mise à jour en incrémentant SystemUpdateID et ContainerUpdateIDs.
/// `container_ids` doit contenir les IDs des conteneurs impactés.
pub fn notify_containers_updated(container_ids: &[&str]) {
    let new_id = SYSTEM_UPDATE_ID
        .fetch_add(1, Ordering::Relaxed)
        .saturating_add(1);
    set_system_update_id(new_id);

    if !container_ids.is_empty() {
        let mut buf = String::new();
        for (idx, cid) in container_ids.iter().enumerate() {
            if idx > 0 {
                buf.push(',');
            }
            buf.push_str(cid);
            buf.push(',');
            buf.push_str(&new_id.to_string());
        }
        set_container_update_ids(&buf);
    }
}

fn set_system_update_id(id: u32) {
    tracing::info!("ContentDirectory: SystemUpdateID -> {}", id);
    if let Some(service) = CONTENTDIR_INSTANCE.get().and_then(|w| w.upgrade()) {
        if let Some(var) = service.get_variable("SystemUpdateID") {
            spawn_set_value(var, StateValue::UI4(id), "SystemUpdateID");
        }
    }
}

fn set_container_update_ids(value: &str) {
    tracing::info!("ContentDirectory: ContainerUpdateIDs -> {}", value);
    {
        let mut guard = CONTAINER_UPDATE_IDS.lock().unwrap();
        *guard = value.to_string();
    }

    if let Some(service) = CONTENTDIR_INSTANCE.get().and_then(|w| w.upgrade()) {
        if let Some(var) = service.get_variable("ContainerUpdateIDs") {
            spawn_set_value(
                var,
                StateValue::String(value.to_string()),
                "ContainerUpdateIDs",
            );
        }
    }
}

fn spawn_set_value(var: Arc<StateVarInstance>, value: StateValue, name: &str) {
    let name = name.to_string();
    task::spawn(async move {
        if let Err(err) = var.set_value(value).await {
            tracing::warn!(
                variable = name.as_str(),
                error = %err,
                "Failed to update ContentDirectory state variable"
            );
        }
    });
}
