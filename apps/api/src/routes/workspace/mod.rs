mod templates;
mod docker_raw;
mod instances;
mod persistent_volumes;
mod registry;

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
