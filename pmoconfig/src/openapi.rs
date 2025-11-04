use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "PMOMusic Configuration API",
        version = "0.1.0",
        description = "API REST pour gérer la configuration de PMOMusic",
        contact(
            name = "PMOMusic Team",
        )
    ),
    paths(
        crate::api::get_full_config,
        crate::api::get_config_value,
        crate::api::update_config_value,
    ),
    components(
        schemas(
            crate::api::ConfigValue,
            crate::api::UpdateConfigRequest,
            crate::api::UpdateConfigResponse,
        )
    ),
    tags(
        (name = "config", description = "Endpoints de gestion de la configuration")
    )
)]
pub struct ApiDoc;
