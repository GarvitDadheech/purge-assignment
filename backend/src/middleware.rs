use actix_web::{
    dev::Payload,
    error::ErrorUnauthorized,
    http, FromRequest, HttpRequest,
};
use serde::{Deserialize, Serialize};
use std::future::{ready, Ready};
use uuid::Uuid;
use crate::auth::decode_jwt;

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthenticatedUser {
    pub id: Uuid,
}

impl FromRequest for AuthenticatedUser {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let auth_header = req.headers().get(http::header::AUTHORIZATION);

        if let Some(auth_header) = auth_header {
            if let Ok(auth_str) = auth_header.to_str() {
                if auth_str.starts_with("Bearer ") {
                    let token = &auth_str[7..];
                    if let Ok(claims) = decode_jwt(token) {
                        return ready(Ok(AuthenticatedUser { id: claims.sub }));
                    }
                }
            }
        }
        ready(Err(ErrorUnauthorized("Invalid token")))
    }
}
