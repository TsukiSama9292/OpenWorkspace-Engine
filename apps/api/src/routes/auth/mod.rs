mod login;
mod session;

use axum::Router;

use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(login::routes())
        .merge(session::routes())
}
