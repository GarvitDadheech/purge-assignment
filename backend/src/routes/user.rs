use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use solana_sdk::signer::{keypair::Keypair, Signer};
use store::user::CreateUserRequest;
use store::Store;
use crate::auth::create_jwt;
use bcrypt::verify;
use crate::middleware::AuthenticatedUser;

#[derive(Deserialize)]
pub struct SignUpRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct SignInRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct UserResponse {
    pub email: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub token: String,
}

#[derive(Serialize)]
pub struct SignupResponse {
    message: String,
}

#[actix_web::post("/signup")]
pub async fn sign_up(
    store: web::Data<Store>,
    req: web::Json<SignUpRequest>,
) -> Result<HttpResponse> {
    let keypair = Keypair::new();
    let public_key = keypair.pubkey().to_string();

    let create_user_request = CreateUserRequest {
        email: req.email.clone(),
        password: req.password.clone(),
        public_key: public_key.clone(),
    };

    match store.create_user(create_user_request).await {
        Ok(_) => {
            if let Err(e) = store.add_public_key(&public_key).await {
                // TODO: Handle this error case more gracefully
                log::error!("Failed to add public key to watch list: {}", e);
            }
            let response = SignupResponse {
                message: "User created successfully".to_string(),
            };
            Ok(HttpResponse::Created().json(response))
        }
        Err(e) => Ok(HttpResponse::InternalServerError().json(e.to_string())),
    }
}

#[actix_web::post("/signin")]
pub async fn sign_in(
    store: web::Data<Store>,
    req: web::Json<SignInRequest>,
) -> Result<HttpResponse> {
    let user = match store.get_user_by_email(&req.email).await {
        Ok(Some(user)) => user,
        Ok(None) => return Ok(HttpResponse::Unauthorized().finish()),
        Err(_) => return Ok(HttpResponse::InternalServerError().finish()),
    };

    match verify(&req.password, &user.password_hash) {
        Ok(true) => {
            let token = create_jwt(user.id).unwrap();
            let response = AuthResponse { token };
            Ok(HttpResponse::Ok().json(response))
        }
        _ => Ok(HttpResponse::Unauthorized().finish()),
    }
}

#[actix_web::get("/user")]
pub async fn get_user(
    store: web::Data<Store>,
    user: AuthenticatedUser,
) -> Result<HttpResponse> {
    match store.get_user_by_id(user.id).await {
        Ok(Some(user)) => {
            let user_response = UserResponse { email: user.email };
            Ok(HttpResponse::Ok().json(user_response))
        }
        Ok(None) => Ok(HttpResponse::NotFound().finish()),
        Err(_) => Ok(HttpResponse::InternalServerError().finish()),
    }
}
