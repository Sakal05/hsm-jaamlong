use crate::handlers::hsm_handler::{exchange_public_key_handler, sign_erc20_transaction_handler, sign_raw_transaction_handler};
use crate::utils::jwt_auth::auth;
use axum::middleware;
use axum::{routing::{get, post}, Router};

pub fn sign_tx_routes() -> Router {
    Router::new()
        .route(
            "/sign-erc20-tx",
            post(sign_erc20_transaction_handler).route_layer(middleware::from_fn(auth)),
        )
        .route(
            "/sign-raw-tx",
            post(sign_raw_transaction_handler).route_layer(middleware::from_fn(auth)),
        )
        .route(
            "/pk",
            get(exchange_public_key_handler).route_layer(middleware::from_fn(auth)),
        )
}
