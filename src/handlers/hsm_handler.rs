use crate::utils::hsm_utils::{sign_erc20, sign_raw_tx, TxBroadcastRequest};
use axum::{http::StatusCode, response::IntoResponse, Json};

pub async fn sign_erc20_transaction_handler(
    Json(payload): Json<TxBroadcastRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    println!(" ========= Payload: {:#?}", &payload);
    let signed_transaction = match sign_erc20(&payload).await {
        Ok(tx) => tx,
        Err(err) => {
            let error_message = format!("Error retrieving origin network: {}", err);
            let json_response = serde_json::json!({
                "status": "fail",
                "data": error_message
            });
            return Ok(Json(json_response));
        }
    };
    let json_response = serde_json::json!({
        "status": "success",
        "data": signed_transaction
    });
    Ok(Json(json_response))
}

pub async fn sign_raw_transaction_handler(
    Json(payload): Json<TxBroadcastRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    println!(" ========= Payload: {:#?}", &payload);
    let signed_transaction = match sign_raw_tx(&payload).await {
        Ok(tx) => tx,
        Err(err) => {
            let error_message = format!("Error retrieving origin network: {}", err);
            let json_response = serde_json::json!({
                "status": "fail",
                "data": error_message
            });
            return Ok(Json(json_response));
        }
    };
    let json_response = serde_json::json!({
        "status": "success",
        "data": signed_transaction
    });
    Ok(Json(json_response))
}
