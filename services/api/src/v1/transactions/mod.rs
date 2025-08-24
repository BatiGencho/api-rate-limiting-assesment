use axum::{extract::DefaultBodyLimit, routing::post, Router};

mod submit;

pub fn router() -> Router<crate::lib::AppState> {
    Router::new().route(
        "/submit",
        post(submit::handler).layer(DefaultBodyLimit::max(5 * 1024 * 1024)),
    ) // 5 Mbs max body size
}
