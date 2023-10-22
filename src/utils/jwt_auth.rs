use axum::{
    http::{header, Request, StatusCode},
    middleware::Next,
    response::IntoResponse,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub status: &'static str,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    sub: String,
    role: String,
    iat: usize,
    exp: usize,
}

pub async fn auth<B>(
    cookie_jar: CookieJar,
    mut req: Request<B>,
    next: Next<B>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Retrieve token from cookie or Authorization header
    let token = cookie_jar
        .get("token")
        .map(|cookie| cookie.value().to_string())
        .or_else(|| {
            req.headers()
                .get(header::AUTHORIZATION)
                .and_then(|auth_header| auth_header.to_str().ok())
                .and_then(|auth_value| {
                    auth_value
                        .strip_prefix("Bearer ")
                        .map(|bearer| bearer.to_owned())
                })
        });

    // Ensure a valid token is present
    let token = token.ok_or_else(|| {
        let json_error = ErrorResponse {
            status: "fail",
            message: "You are not logged in, please provide token".to_string(),
        };
        (StatusCode::UNAUTHORIZED, Json(json_error))
    })?;
    // Decode token and extract claims
    let secret_key = dotenvy::var("SECRET_KEY").expect("Secret key cannot be empty");
    let pk = dotenvy::var("PUBLIC_KEY").expect("Secret key cannot be empty");
    let claims = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(secret_key.as_ref()),
        &Validation::default(),
    )
    .map_err(|e| {
        println!("Error: {}", e);
        let json_error = ErrorResponse {
            status: "fail",
            message: "Invalid token".to_string(),
        };
        (StatusCode::UNAUTHORIZED, Json(json_error))
    })?
    .claims;
    let pub_key = claims.sub;
    if claims.role != "admin" {
        let json_error = ErrorResponse {
            status: "fail",
            message: "Invalid not equal token".to_string(),
        };
        return Ok(Err((StatusCode::UNAUTHORIZED, Json(json_error))));
    }

    //Check user ID from token against stored user ID
    if pub_key.ne(&pk) {
        let json_error = ErrorResponse {
            status: "fail",
            message: "Invalid not equal token".to_string(),
        };
        Ok(Err((StatusCode::UNAUTHORIZED, Json(json_error))))
    } else {
        // Insert username into request extensions
        req.extensions_mut().insert(pub_key);
        Ok(Ok(next.run(req).await))
    }
}
