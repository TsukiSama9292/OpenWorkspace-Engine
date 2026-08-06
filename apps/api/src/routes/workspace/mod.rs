pub(crate) mod templates;
pub(crate) mod docker_raw;
pub(crate) mod instances;
pub(crate) mod persistent_volumes;
pub(crate) mod registry;

use axum::Router;

use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(templates::routes())
        .merge(instances::routes())
        .merge(docker_raw::routes())
        .merge(persistent_volumes::routes())
        .merge(registry::routes())
}
