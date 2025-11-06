use actix_web::{web::{self, post, Json}, App, HttpResponse, HttpServer, Responder};
use db::{MpcKey, MpcStore};
use dotenv::dotenv;
use error::Error;
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    hash::Hash,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::{Transaction, Message},
    system_instruction,
};
use std::{str::FromStr, sync::Arc};
use store::Store;
use uuid::Uuid;

use crate::serialization::{AggMessage1, PartialSignature, SecretAggStepOne};

pub mod db;
pub mod error;
pub mod serialization;
pub mod tss;

#[derive(Serialize)]
struct GenerateResponse {
    end_user_pubkey: String,
    node1_pubkey: String,
    node2_pubkey: String,
}

#[derive(Deserialize)]
struct AggregateKeysRequest {
    pubkeys: Vec<String>,
}

#[derive(Serialize)]
struct AggregateKeysResponse {
    aggregated_pubkey: String,
}

#[derive(Deserialize)]
struct AggSendStep1Request {
    end_user_pubkey: String,
    node_id: i32,
    to: String,
    amount: f64,
    memo: Option<String>,
}

#[derive(Serialize)]
struct AggSendStep1Response {
    session_id: Uuid,
    agg_message_1: AggMessage1,
}

#[derive(Deserialize)]
struct AggSendStep2Request {
    session_id: Uuid,
    node_id: i32,
    agg_message_1: AggMessage1,
}

#[derive(Serialize)]
struct AggSendStep2Response {
    partial_signature: PartialSignature,
    agg_message_2: AggMessage1,
}

#[derive(Deserialize)]
struct AggregateSignaturesRequest {
    session_id: Uuid,
    partial_signature_2: PartialSignature,
    agg_message_2: AggMessage1,
}

#[derive(Serialize)]
struct AggregateSignaturesResponse {
    transaction_signature: String,
}

struct AppState {
    mpc_store_1: MpcStore,
    mpc_store_2: MpcStore,
    main_store: Arc<Store>,
    rpc_client: RpcClient,
}

impl AppState {
    fn get_mpc_store(&self, node_id: i32) -> Result<&MpcStore, Error> {
        match node_id {
            1 => Ok(&self.mpc_store_1),
            2 => Ok(&self.mpc_store_2),
            _ => Err(Error::InvalidRequest("Invalid node_id".to_string())),
        }
    }
}

async fn generate(
    app_state: web::Data<AppState>,
) -> Result<impl Responder, Error> {
    let mut rng = rand::thread_rng();
    let kp1 = Keypair::new(&mut rng);
    let kp2 = Keypair::new(&mut rng);

    let pubkeys = vec![kp1.pubkey(), kp2.pubkey()];
    let agg_pk = tss::key_agg(pubkeys, None).unwrap();
    let end_user_pubkey = Pubkey::new_from_array(agg_pk.agg_public_key.to_bytes(true)).to_string();

    let mpc_store_1 = app_state.get_mpc_store(1)?;
    let mpc_store_2 = app_state.get_mpc_store(2)?;

    let mpc_key1 = MpcKey {
        end_user_pubkey: end_user_pubkey.clone(),
        node_id: 1,
        public_key: kp1.pubkey().to_string(),
        private_key: bs58::encode(kp1.to_bytes()).into_string(),
    };
    let mpc_key2 = MpcKey {
        end_user_pubkey: end_user_pubkey.clone(),
        node_id: 2,
        public_key: kp2.pubkey().to_string(),
        private_key: bs58::encode(kp2.to_bytes()).into_string(),
    };

    mpc_store_1.store_key(&mpc_key1).await?;
    mpc_store_2.store_key(&mpc_key2).await?;

    app_state.main_store.add_public_key(&end_user_pubkey).await.unwrap();

    Ok(Json(GenerateResponse {
        end_user_pubkey,
        node1_pubkey: kp1.pubkey().to_string(),
        node2_pubkey: kp2.pubkey().to_string(),
    }))
}

async fn aggregate_keys(req: Json<AggregateKeysRequest>) -> Result<impl Responder, Error> {
    let pubkeys: Result<Vec<Pubkey>, _> = req
        .pubkeys
        .iter()
        .map(|s| Pubkey::from_str(s))
        .collect();
    let pubkeys = pubkeys.map_err(|_| Error::InvalidRequest("Invalid pubkey provided".to_string()))?;
    let agg_pk = tss::key_agg(pubkeys, None).unwrap();
    let aggregated_pubkey = Pubkey::new_from_array(agg_pk.agg_public_key.to_bytes(true)).to_string();
    Ok(Json(AggregateKeysResponse { aggregated_pubkey }))
}

async fn agg_send_step1(
    app_state: web::Data<AppState>,
    req: Json<AggSendStep1Request>,
) -> Result<impl Responder, Error> {
    let mpc_store = app_state.get_mpc_store(req.node_id)?;
    let key = mpc_store.get_key(&req.end_user_pubkey, req.node_id).await?;
    let keypair = Keypair::from_bytes(&bs58::decode(key.private_key).into_vec().unwrap()).unwrap();

    let (agg_message_1, secret_state_1) = tss::step_one(keypair);
    let session_id = mpc_store
        .create_session(
            &req.end_user_pubkey,
            &secret_state_1,
            &req.to,
            req.amount,
            req.memo.clone(),
            None, // No generic transaction for SOL send
        )
        .await?;
    
    Ok(Json(AggSendStep1Response { session_id, agg_message_1 }))
}

async fn agg_send_step2(
    app_state: web::Data<AppState>,
    req: Json<AggSendStep2Request>,
) -> Result<impl Responder, Error> {
    let mpc_store = app_state.get_mpc_store(req.node_id)?;
    let session = mpc_store.get_session(req.session_id).await?;
    let key = mpc_store.get_key(&session.end_user_pubkey, req.node_id).await?;
    let keypair = Keypair::from_bytes(&bs58::decode(key.private_key).into_vec().unwrap()).unwrap();

    let (agg_message_2, secret_state_2) = tss::step_one(keypair);

    let keys_from_db = mpc_store.get_keys_for_user(&session.end_user_pubkey).await?;
    let pubkeys: Vec<Pubkey> = keys_from_db
        .iter()
        .map(|k| Pubkey::from_str(&k.public_key).unwrap())
        .collect();
    
    let rpc_client = &app_state.rpc_client;
    let recent_blockhash = rpc_client.get_latest_blockhash()?;

    let message = if let Some(tx_str) = session.transaction {
        let tx: Transaction = serde_json::from_str(&tx_str).unwrap();
        tx.message_data()
    } else {
        // Create SOL transfer message
        let to_pubkey = Pubkey::from_str(&session.to_address).unwrap();
        let from_pubkey = Pubkey::from_str(&keys_from_db.iter().find(|k| k.node_id == req.node_id).unwrap().public_key).unwrap();
        let ix = system_instruction::transfer(&from_pubkey, &to_pubkey, (session.amount * 1e9) as u64);
        let mut msg = Message::new(&[ix], Some(&from_pubkey));
        msg.recent_blockhash = recent_blockhash;
        msg.serialize()
    };
    
    let partial_signature = tss::step_two(
        keypair,
        &message,
        pubkeys,
        vec![req.agg_message_1.clone()],
        secret_state_2.clone(),
    ).unwrap();

    mpc_store
        .update_session_with_step2_data(
            req.session_id,
            &secret_state_2,
            &bs58::encode(partial_signature.0.as_ref()).into_string(),
            &serde_json::to_string(&agg_message_2).unwrap(),
        )
        .await?;

    Ok(Json(AggSendStep2Response { partial_signature, agg_message_2 }))
}


async fn aggregate_signatures_broadcast(
    app_state: web::Data<AppState>,
    req: Json<AggregateSignaturesRequest>,
) -> Result<impl Responder, Error> {
    let mpc_store_1 = app_state.get_mpc_store(1)?;
    let session = mpc_store_1.get_session(req.session_id).await?;
    let keys_from_db = mpc_store_1.get_keys_for_user(&session.end_user_pubkey).await?;
    let key1 = &keys_from_db[0];
    let keypair1 = Keypair::from_bytes(&bs58::decode(&key1.private_key).into_vec().unwrap()).unwrap();
    
    let pubkeys: Vec<Pubkey> = keys_from_db
        .iter()
        .map(|k| Pubkey::from_str(&k.public_key).unwrap())
        .collect();

    let rpc_client = &app_state.rpc_client;
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    
    let secret_state_1: SecretAggStepOne = serde_json::from_slice(&session.secret_state_1.unwrap()).unwrap();

    let tx = if let Some(tx_str) = session.transaction {
        serde_json::from_str(&tx_str).unwrap()
    } else {
        let agg_pubkey = tss::key_agg(pubkeys.clone(), None).unwrap().agg_public_key;
        let agg_pubkey = Pubkey::new_from_array(agg_pubkey.to_bytes(true));
        let to_pubkey = Pubkey::from_str(&session.to_address).unwrap();
        let ix = system_instruction::transfer(&agg_pubkey, &to_pubkey, (session.amount * 1e9) as u64);
        let mut message = Message::new(&[ix], Some(&agg_pubkey));
        message.recent_blockhash = recent_blockhash;
        Transaction::new_unsigned(message)
    };

    let partial_signature_1 = tss::step_two(
        keypair1,
        &tx.message_data(),
        pubkeys.clone(),
        vec![req.agg_message_2.clone()],
        secret_state_1,
    ).unwrap();

    let final_tx = tss::sign_and_broadcast_transaction(
        tx,
        pubkeys,
        vec![partial_signature_1, req.partial_signature_2],
    ).unwrap();

    let tx_sig = rpc_client.send_and_confirm_transaction(&final_tx)?;

    Ok(Json(AggregateSignaturesResponse { transaction_signature: tx_sig.to_string() }))
}

async fn send_single() -> Result<HttpResponse, Error> {
    // Implementation can be added here for testing
    Ok(HttpResponse::Ok().body("Not Implemented"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::init();

    let mpc_database_url_1 = std::env::var("MPC_DATABASE_URL_1").expect("MPC_DATABASE_URL_1 must be set");
    let mpc_database_url_2 = std::env::var("MPC_DATABASE_URL_2").expect("MPC_DATABASE_URL_2 must be set");
    let main_database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let rpc_url = std::env::var("SOLANA_RPC_URL").expect("SOLANA_RPC_URL must be set");

    let mpc_pool_1 = sqlx::PgPool::connect(&mpc_database_url_1).await.unwrap();
    let mpc_pool_2 = sqlx::PgPool::connect(&mpc_database_url_2).await.unwrap();
    let main_pool = sqlx::PgPool::connect(&main_database_url).await.unwrap();

    let app_state = web::Data::new(AppState {
        mpc_store_1: MpcStore::new(mpc_pool_1),
        mpc_store_2: MpcStore::new(mpc_pool_2),
        main_store: Arc::new(Store::new(main_pool)),
        rpc_client: RpcClient::new(rpc_url),
    });

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/generate", post().to(generate))
            .route("/send-single", post().to(send_single))
            .route("/aggregate-keys", post().to(aggregate_keys))
            .route("/agg-send-step1", post().to(agg_send_step1))
            .route("/agg-send-step2", post().to(agg_send_step2))
            .route(
                "/aggregate-signatures-broadcast",
                post().to(aggregate_signatures_broadcast),
            )
    })
    .bind("127.0.0.1:8081")? // Running on a different port
    .run()
    .await
}