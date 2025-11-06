use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::middleware::AuthenticatedUser;
use store::Store;
use mpc::serialization::{AggMessage1, PartialSignature};

#[derive(Deserialize)]
pub struct QuoteRequest {
    #[serde(rename = "inputMint")]
    pub input_mint: String,
    #[serde(rename = "outputMint")]
    pub output_mint: String,
    #[serde(rename = "inAmount")]
    pub in_amount: u64,
}

#[derive(Serialize, Deserialize)]
pub struct QuoteResponse {
    #[serde(rename = "outAmount")]
    pub out_amount: String,
    pub id: Uuid,
}

#[derive(Deserialize)]
pub struct SwapRequest {
    pub id: Uuid,
}

#[derive(Serialize)]
pub struct SwapResponse {
    #[serde(rename = "swapTransaction")]
    pub swap_transaction: String,
}

#[derive(Serialize)]
pub struct BalanceResponse {
    pub balance: u64,
}

#[derive(Serialize, Clone)]
pub struct TokenBalance {
    pub balance: u64,
    #[serde(rename = "tokenMint")]
    pub token_mint: String,
    pub symbol: String,
    pub decimals: i32,
}

#[derive(Serialize)]
pub struct TokenBalanceResponse {
    pub balances: Vec<TokenBalance>,
}

#[derive(Deserialize)]
pub struct SendRequest {
    pub to: String,
    pub amount: u64,
    pub mint: Option<String>,
}

#[derive(Serialize)]
pub struct SendResponse {
    pub signature: String,
}

#[derive(Serialize)]
struct JupiterSwapRequest {
    #[serde(rename = "userPublicKey")]
    user_public_key: String,
    #[serde(rename = "quoteResponse")]
    quote_response: serde_json::Value,
}

#[actix_web::post("/quote")]
pub async fn quote(
    store: web::Data<Store>,
    user: AuthenticatedUser,
    req: web::Json<QuoteRequest>,
) -> Result<HttpResponse> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://lite-api.jup.ag/v6/quote?inputMint={}&outputMint={}&amount={}&slippageBps=50",
        req.input_mint, req.output_mint, req.in_amount
    );

    match client.get(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<serde_json::Value>().await {
                    Ok(quote_response) => {
                        match store
                            .create_quote(user.id, quote_response.clone())
                            .await
                        {
                            Ok(stored_quote) => {
                                let out_amount = quote_response["outAmount"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .to_string();
                                let response = QuoteResponse {
                                    out_amount,
                                    id: stored_quote.id,
                                };
                                Ok(HttpResponse::Ok().json(response))
                            }
                            Err(_) => Ok(HttpResponse::InternalServerError().finish()),
                        }
                    }
                    Err(_) => Ok(HttpResponse::InternalServerError().finish()),
                }
            } else {
                Ok(HttpResponse::InternalServerError().finish())
            }
        }
        Err(_) => Ok(HttpResponse::InternalServerError().finish()),
    }
}

#[actix_web::post("/swap")]
pub async fn swap(
    store: web::Data<Store>,
    user: AuthenticatedUser,
    req: web::Json<SwapRequest>,
) -> Result<HttpResponse> {
    let user_model = match store.get_user_by_id(user.id).await {
        Ok(Some(user)) => user,
        _ => return Ok(HttpResponse::InternalServerError().finish()),
    };

    let quote = match store.get_quote(req.id).await {
        Ok(Some(quote)) => quote,
        _ => return Ok(HttpResponse::NotFound().finish()),
    };

    let swap_request_body = JupiterSwapRequest {
        user_public_key: user_model.public_key.clone(),
        quote_response: quote.quote_response,
    };

    let client = reqwest::Client::new();
    let url = "https://lite-api.jup.ag/v6/swap";

    let jupiter_res = client.post(url).json(&swap_request_body).send().await.unwrap().json::<serde_json::Value>().await.unwrap();
    let swap_transaction = jupiter_res["swapTransaction"].as_str().unwrap().to_string();

    // Now sign the transaction with MPC
    let mpc_service_url = std::env::var("MPC_SERVICE_URL").expect("MPC_SERVICE_URL must be set");

    // Step 1: Call agg-send-step1 on node 1
    let step1_req = serde_json::json!({
        "end_user_pubkey": user_model.public_key,
        "node_id": 1,
        "to": "11111111111111111111111111111111", // Placeholder
        "amount": 0, // Placeholder
        "transaction": swap_transaction
    });

    let step1_res = client
        .post(format!("{}/agg-send-step1", mpc_service_url))
        .json(&step1_req)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    let session_id: Uuid = serde_json::from_value(step1_res["session_id"].clone()).unwrap();
    let agg_message_1: AggMessage1 = serde_json::from_value(step1_res["agg_message_1"].clone()).unwrap();

    // Step 2: Call agg-send-step2 on node 2
    let step2_req = serde_json::json!({
        "session_id": session_id,
        "node_id": 2,
        "agg_message_1": agg_message_1
    });

    let step2_res = client
        .post(format!("{}/agg-send-step2", mpc_service_url))
        .json(&step2_req)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    
    let partial_signature_2: PartialSignature = serde_json::from_value(step2_res["partial_signature"].clone()).unwrap();
    let agg_message_2: AggMessage1 = serde_json::from_value(step2_res["agg_message_2"].clone()).unwrap();

    // Step 3: Call aggregate-signatures-broadcast on node 1
    let broadcast_req = serde_json::json!({
        "session_id": session_id,
        "partial_signature_2": partial_signature_2,
        "agg_message_2": agg_message_2
    });

    let broadcast_res = client
        .post(format!("{}/aggregate-signatures-broadcast", mpc_service_url))
        .json(&broadcast_req)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    
    let signature = broadcast_res["transaction_signature"].as_str().unwrap().to_string();

    Ok(HttpResponse::Ok().json(SwapResponse { swap_transaction: signature }))
}

#[actix_web::post("/send")]
pub async fn send(
    store: web::Data<Store>,
    user: AuthenticatedUser,
    req: web::Json<SendRequest>,
) -> Result<HttpResponse> {
    let user_model = store.get_user_by_id(user.id).await.unwrap().unwrap();
    let mpc_service_url = std::env::var("MPC_SERVICE_URL").expect("MPC_SERVICE_URL must be set");

    let client = reqwest::Client::new();

    // Step 1: Call agg-send-step1 on node 1
    let step1_req = serde_json::json!({
        "end_user_pubkey": user_model.public_key,
        "node_id": 1,
        "to": req.to,
        "amount": req.amount as f64 / 1e9, // Convert lamports to SOL
        "memo": req.mint
    });

    let step1_res = client
        .post(format!("{}/agg-send-step1", mpc_service_url))
        .json(&step1_req)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    let session_id: Uuid = serde_json::from_value(step1_res["session_id"].clone()).unwrap();
    let agg_message_1: AggMessage1 = serde_json::from_value(step1_res["agg_message_1"].clone()).unwrap();

    // Step 2: Call agg-send-step2 on node 2
    let step2_req = serde_json::json!({
        "session_id": session_id,
        "node_id": 2,
        "agg_message_1": agg_message_1
    });

    let step2_res = client
        .post(format!("{}/agg-send-step2", mpc_service_url))
        .json(&step2_req)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    
    let partial_signature_2: PartialSignature = serde_json::from_value(step2_res["partial_signature"].clone()).unwrap();
    let agg_message_2: AggMessage1 = serde_json::from_value(step2_res["agg_message_2"].clone()).unwrap();

    // Step 3: Call aggregate-signatures-broadcast on node 1
    let broadcast_req = serde_json::json!({
        "session_id": session_id,
        "partial_signature_2": partial_signature_2,
        "agg_message_2": agg_message_2
    });

    let broadcast_res = client
        .post(format!("{}/aggregate-signatures-broadcast", mpc_service_url))
        .json(&broadcast_req)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    let signature = broadcast_res["transaction_signature"].as_str().unwrap().to_string();

    Ok(HttpResponse::Ok().json(SendResponse { signature }))
}

#[actix_web::get("/balance/sol")]
pub async fn sol_balance(
    store: web::Data<Store>,
    user: AuthenticatedUser,
) -> Result<HttpResponse> {
    match store.get_sol_balance(user.id).await {
        Ok(Some(balance)) => {
            let response = BalanceResponse {
                balance: balance.amount as u64,
            };
            Ok(HttpResponse::Ok().json(response))
        }
        Ok(None) => {
            let response = BalanceResponse { balance: 0 };
            Ok(HttpResponse::Ok().json(response))
        }
        Err(_) => Ok(HttpResponse::InternalServerError().finish()),
    }
}

#[actix_web::get("/balance/tokens")]
pub async fn token_balance(
    store: web::Data<Store>,
    user: AuthenticatedUser,
) -> Result<HttpResponse> {
    match store.get_token_balances(user.id).await {
        Ok(balances) => {
            let token_balances = balances
                .into_iter()
                .map(|(balance, asset)| TokenBalance {
                    balance: balance.amount as u64,
                    token_mint: asset.mint_address,
                    symbol: asset.symbol,
                    decimals: asset.decimals,
                })
                .collect();
            let response = TokenBalanceResponse {
                balances: token_balances,
            };
            Ok(HttpResponse::Ok().json(response))
        }
        Err(_) => Ok(HttpResponse::InternalServerError().finish()),
    }
}
