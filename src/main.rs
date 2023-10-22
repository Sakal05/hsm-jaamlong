pub mod handlers;
pub mod utils;

use axum::{
    http::{
        header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
        HeaderValue, Method,
    },
    routing::post, Router,
};
use crate::handlers::hsm_handler::{sign_erc20_transaction_handler, sign_raw_transaction_handler};
use tokio::task;
use tower_http::cors::CorsLayer;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let path = dotenvy::var("PATH_RAW")?;
    let cors = CorsLayer::new()
        .allow_origin(path.parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
        .allow_credentials(true)
        .allow_headers([AUTHORIZATION, ACCEPT, CONTENT_TYPE]);
    let app = Router::new()
        .route("/sign-erc20-tx", post(sign_erc20_transaction_handler))
        .route("/sign-raw-tx", post(sign_raw_transaction_handler))
        .layer(cors);
    println!("🚀 HSM Server started successfully, port {}", &path[path.len() - 4..]);
    let server = task::spawn(async move {
        axum::Server::bind(&format!("0.0.0.0:{}", &path[path.len() - 4..]).parse().unwrap())
            .serve(app.into_make_service())
            .await
            .unwrap();
    });
    server.await.unwrap();

    Ok(())
}