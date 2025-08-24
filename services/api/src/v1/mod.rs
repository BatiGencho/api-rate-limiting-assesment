use axum::Router;

mod transactions;

pub fn router() -> Router<crate::lib::AppState> {
    Router::new().nest("/transactions", transactions::router())
}
