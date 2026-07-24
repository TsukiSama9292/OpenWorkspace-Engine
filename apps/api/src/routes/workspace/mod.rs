mod configs;
mod docker_raw;
mod instances;
mod registry;

use axum::Router;

use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(configs::routes())
        .merge(instances::routes())
        .merge(docker_raw::routes())
        .merge(registry::routes())
}
