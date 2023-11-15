pub mod handlers;
pub mod routes;
pub mod utils;

use crate::routes::hsm_router;
use axum::{
    http::{
        header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
        HeaderValue, Method,
    },
    routing::Router,
};
use tokio::task;
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    // HSM Server
    let hsm_domain = dotenvy::var("BRIDGE_DOMAIN").expect("HSM Domain not found");
    let hsm_port = dotenvy::var("HSM_PORT").expect("HSM Port not found");
    let bridge_port = dotenvy::var("BRIDGE_PORT").expect("HSM Port not found");

    let cors = CorsLayer::new()
        .allow_origin(
            format!("{}:{}", hsm_domain, bridge_port)
                .parse::<HeaderValue>()
                .unwrap(),
        )
        .allow_methods([Method::GET, Method::POST])
        .allow_credentials(true)
        .allow_headers([AUTHORIZATION, ACCEPT, CONTENT_TYPE]);
    let app = Router::new()
        .merge(hsm_router::sign_tx_routes())
        .layer(cors);

    println!("🚀 HSM Server started successfully, port {}", hsm_port);

    let server = task::spawn(async move {
        axum::Server::bind(&format!("0.0.0.0:{}", hsm_port).parse().unwrap())
            .serve(app.into_make_service())
            .await
            .unwrap();
    });
    tracing::info!("Received request: {:?}", &server);
    server.await.unwrap();

    Ok(())
}
