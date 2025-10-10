//! API REST pour l'introspection UPnP.
//!
//! Ce module fournit des endpoints HTTP pour explorer et modifier
//! l'état du serveur UPnP en temps réel, similaire à pmolog et pmocovers.
//!
//! # Routes disponibles
//!
//! - `GET /api/upnp/devices` - Liste tous les devices
//! - `GET /api/upnp/devices/:udn` - Détails d'un device
//! - `GET /api/upnp/devices/:udn/services/:service/variables` - Variables d'un service

use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use crate::{
    state_variables::UpnpVariable,
    upnp_server,
    UpnpTyped, UpnpTypedInstance,
};
use pmoserver::Server;
use serde_json::json;
use tracing::info;

/// Handler : Liste tous les devices UPnP.
///
/// GET /api/upnp/devices
async fn list_devices() -> impl IntoResponse {
    upnp_server::with_devices(|devices| {
        let device_list: Vec<_> = devices
            .iter()
            .map(|d| {
                json!({
                    "udn": d.udn(),
                    "name": d.get_name(),
                    "friendly_name": d.get_model().friendly_name(),
                    "device_type": d.get_model().device_type(),
                    "manufacturer": d.get_model().manufacturer(),
                    "model_name": d.get_model().model_name(),
                    "base_url": d.base_url(),
                    "description_url": format!("{}{}", d.base_url(), d.description_route()),
                })
            })
            .collect();

        Json(json!({
            "count": devices.len(),
            "devices": device_list
        }))
    })
}

/// Handler : Détails d'un device UPnP.
///
/// GET /api/upnp/devices/:udn
async fn get_device(Path(udn): Path<String>) -> impl IntoResponse {
    match upnp_server::get_device_by_udn(&udn) {
        Some(device) => {
            let model = device.get_model();
            let services: Vec<_> = device
                .services()
                .iter()
                .map(|s| {
                    // Collecter les actions
                    let actions: Vec<_> = s.actions()
                        .all()
                        .iter()
                        .map(|a| {
                            let all_args = a.arguments_set().all();

                            let in_args: Vec<_> = all_args
                                .iter()
                                .filter(|arg| arg.get_model().is_in())
                                .map(|arg| {
                                    let model = arg.get_model();
                                    json!({
                                        "name": arg.get_name(),
                                        "related_state_variable": model.state_variable().get_name()
                                    })
                                })
                                .collect();

                            let out_args: Vec<_> = all_args
                                .iter()
                                .filter(|arg| arg.get_model().is_out())
                                .map(|arg| {
                                    let model = arg.get_model();
                                    json!({
                                        "name": arg.get_name(),
                                        "related_state_variable": model.state_variable().get_name()
                                    })
                                })
                                .collect();

                            json!({
                                "name": a.get_name(),
                                "in_arguments": in_args,
                                "out_arguments": out_args
                            })
                        })
                        .collect();

                    json!({
                        "name": s.get_name(),
                        "service_type": s.service_type(),
                        "service_id": s.service_id(),
                        "control_url": format!("{}{}", device.base_url(), s.control_route()),
                        "event_url": format!("{}{}", device.base_url(), s.event_route()),
                        "scpd_url": format!("{}{}", device.base_url(), s.scpd_route()),
                        "actions": actions
                    })
                })
                .collect();

            (
                StatusCode::OK,
                Json(json!({
                    "udn": device.udn(),
                    "name": device.get_name(),
                    "friendly_name": model.friendly_name(),
                    "device_type": model.device_type(),
                    "manufacturer": model.manufacturer(),
                    "model_name": model.model_name(),
                    "base_url": device.base_url(),
                    "description_url": format!("{}{}", device.base_url(), device.description_route()),
                    "services": services,
                })),
            )
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "Device not found",
                "udn": udn
            })),
        ),
    }
}

/// Handler : Variables d'un service.
///
/// GET /api/upnp/devices/:udn/services/:service/variables
async fn get_service_variables(Path((udn, service_name)): Path<(String, String)>) -> impl IntoResponse {
    match upnp_server::get_device_by_udn(&udn) {
        Some(device) => match device.get_service(&service_name) {
            Some(service) => {
                let variables: Vec<_> = service
                    .statevariables()
                    .all()
                    .iter()
                    .map(|v| {
                        let model = v.get_model();

                        // Obtenir les allowed values
                        let allowed_values = {
                            let av = model.get_allowed_values();
                            if av.is_empty() {
                                None
                            } else {
                                Some(av.iter().map(|val| val.to_string()).collect::<Vec<_>>())
                            }
                        };

                        // Accéder au range si défini
                        let (min, max) = if let Some(range) = model.get_range() {
                            (
                                Some(range.get_minimum().to_string()),
                                Some(range.get_maximum().to_string())
                            )
                        } else {
                            (None, None)
                        };

                        json!({
                            "name": v.get_name(),
                            "value": v.value().to_string(),
                            "data_type": model.get_data_type().to_string(),
                            "sends_events": v.is_sending_notification(),
                            "default_value": model.get_default_value().map(|dv| dv.to_string()),
                            "allowed_values": allowed_values,
                            "min": min,
                            "max": max,
                            "step": model.get_step().map(|s| s.to_string()),
                        })
                    })
                    .collect();

                (
                    StatusCode::OK,
                    Json(json!({
                        "udn": udn,
                        "service": service_name,
                        "variables": variables
                    })),
                )
            }
            None => (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "Service not found",
                    "service": service_name
                })),
            ),
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "Device not found",
                "udn": udn
            })),
        ),
    }
}

/// Trait d'extension pour enregistrer l'API UPnP sur un serveur.
///
/// Similaire à `WebAppExt` et `CoverCacheExt`.
pub trait UpnpApiExt {
    /// Enregistre l'API REST d'introspection UPnP.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// server.register_upnp_api().await;
    /// ```
    async fn register_upnp_api(&mut self);
}

impl UpnpApiExt for Server {
    async fn register_upnp_api(&mut self) {
        info!("📡 Registering UPnP introspection API...");

        // Créer le routeur Axum
        let app = Router::new()
            .route("/devices", get(list_devices))
            .route("/devices/{udn}", get(get_device))
            .route(
                "/devices/{udn}/services/{service}/variables",
                get(get_service_variables),
            );

        // Monter le routeur sur /api/upnp via add_router
        self.add_router("/api/upnp", app).await;

        info!("✅ UPnP API registered:");
        info!("   - GET /api/upnp/devices");
        info!("   - GET /api/upnp/devices/:udn");
        info!("   - GET /api/upnp/devices/:udn/services/:service/variables");
    }
}
