use crate::utils::{
    encryption,
    hsm_utils::{
        hsm_generate_pk, sign_erc20, sign_raw_tx, verify_signature, SignTx, TxBroadcastRequest,
        TxRequestTest,
    },
};
use axum::{http::StatusCode, response::IntoResponse, Json};
use redis::Commands;
use serde::Deserialize;

pub async fn sign_erc20_transaction_handler(
    Json(payload): Json<TxRequestTest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let deser_payload: SignTx =
        serde_json::from_str(&payload.sign_tx).expect("Error parsing payload to SignTx Struct");
    // println!("Deserialized payload: {:?}", deser_payload);
    let red_path = dotenvy::var("RED_URL").expect("Redis URL not found");
    let client = redis::Client::open(red_path).expect("Client not found");
    let mut con = client.get_connection().expect("Connection failed");
    let sk: Vec<u8> = con.get(payload.pk).expect("Key value not found");

    let decrypted_payload = encryption::decrypt(&deser_payload.message, &sk);
    println!("Decrypted payload: {:?}", decrypted_payload);
    let tx_field: TxBroadcastRequest = serde_json::from_str(&decrypted_payload)
        .expect("Error parsing decrypted payload to Broadcast Tx Struct");
    // println!("Tx field after decrypted: {:?}", tx_field);
    // ======== perform verification on the payload
    if verify_signature(&deser_payload) {
        println!("Verified Passed")
    } else {
        let error_message = "Signature verification failed";
        let json_response = serde_json::json!({
            "status": "fail",
            "data": error_message
        });
        return Ok(Json(json_response));
    }

    let signed_transaction = match sign_erc20(&tx_field).await {
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

    let message = signed_transaction.message;
    let r_tx = signed_transaction.r_tx;
    let signature = signed_transaction.signature;
    println!("Raw transaction: {:?}", r_tx);
    // perform encryption
    let s_tx_string = serde_json::to_string(&r_tx).expect("Failed to serialize transaction");
    let encrypted_sign_tx = encryption::encrypt(&s_tx_string, &sk);
    println!("encrypted bridge data: {:?}", encrypted_sign_tx);

    let json_response = serde_json::json!({
        "status": "success",
        "data": encrypted_sign_tx,
        "message": message,
        "signature": signature,
    });
    Ok(Json(json_response))
}

pub async fn sign_raw_transaction_handler(
    Json(payload): Json<TxRequestTest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    println!(" ========= Payload: {:#?}", &payload);
    let deser_payload: SignTx =
        serde_json::from_str(&payload.sign_tx).expect("Error parsing payload to SignTx Struct");
    // println!("Deserialized payload: {:?}", deser_payload);
    let red_path = dotenvy::var("RED_URL").expect("Redis URL not found");
    let client = redis::Client::open(red_path).expect("Client not found");
    let mut con = client.get_connection().expect("Connection failed");
    let sk: Vec<u8> = con.get(payload.pk).expect("Key value not found");

    let decrypted_payload = encryption::decrypt(&deser_payload.message, &sk);
    println!("Decrypted payload: {:?}", decrypted_payload);
    let tx_field: TxBroadcastRequest = serde_json::from_str(&decrypted_payload)
        .expect("Error parsing decrypted payload to Broadcast Tx Struct");

    // ======== perform verification on the payload
    if verify_signature(&deser_payload) {
        println!("Verified Passed")
    } else {
        let error_message = "Signature verification failed";
        let json_response = serde_json::json!({
            "status": "fail",
            "data": error_message
        });
        return Ok(Json(json_response));
    }
    let signed_transaction = match sign_raw_tx(&tx_field).await {
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

#[derive(Debug, Deserialize)]
pub struct Pk {
    pk: Vec<u8>,
}

pub async fn exchange_public_key_handler(
    Json(pk): Json<Pk>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // let payload: Vec<u8> = serde_json::f(&pk).unwrap();
    println!("Received public key: {:?}", &pk.pk);
    let pk = hsm_generate_pk(&pk.pk);
    let pk_hex = format!("0x{}", hex::encode(&pk.0));

    println!("PK: {:?}", &pk_hex);
    let json_response = serde_json::json!({
        "status": "success",
        "data": pk.0
    });
    Ok(Json(json_response))
}
