use dotenv::dotenv;
use futures::StreamExt;
use log::{error, info};
use spl_token::state::Account as TokenAccount;
use sqlx::PgPool;
use std::{collections::HashMap, env, str::FromStr};
use store::Store;
use yellowstone::GeyserGrpcClient;
use yellowstone_grpc_proto::prelude::{subscribe_update::UpdateOneof, SubscribeUpdateAccount};

pub mod yellowstone;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    env_logger::init();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let triton_api_token = env::var("TRITON_API_TOKEN").expect("TRITON_API_TOKEN must be set");

    let pool = PgPool::connect(&database_url).await?;
    let store = Store::new(pool);

    let public_keys = store.get_all_public_keys().await?;
    let addresses_to_monitor: Vec<String> = public_keys
        .into_iter()
        .map(|pk| pk.end_user_pubkey)
        .collect();

    if addresses_to_monitor.is_empty() {
        info!("No public keys to monitor. Exiting.");
        return Ok(());
    }

    info!("Monitoring {} addresses", addresses_to_monitor.len());

    let mut client = GeyserGrpcClient::build_from_static("https://grpc.triton.one:443")
        .x_token(Some(&triton_api_token))?
        .connect()
        .await?;

    let (_sink, mut stream) = client
        .subscribe_to_addresses(addresses_to_monitor.clone())
        .await?;

    info!("Successfully subscribed to addresses. Waiting for updates...");

    let addresses_set: std::collections::HashSet<String> =
        addresses_to_monitor.into_iter().collect();

    while let Some(update) = stream.next().await {
        match update {
            Ok(update) => {
                if let Some(UpdateOneof::Account(account_update)) = update.update_oneof {
                    if let Err(e) = handle_account_update(&store, &addresses_set, account_update).await {
                        error!("Error handling account update: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Stream error: {}", e);
            }
        }
    }

    Ok(())
}

async fn handle_account_update(
    store: &Store,
    monitored_addresses: &std::collections::HashSet<String>,
    account_update: SubscribeUpdateAccount,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(account) = account_update.account {
        let pubkey_str = bs58::encode(&account.pubkey).into_string();

        // Check if this is a direct SOL balance update for one of our users
        if monitored_addresses.contains(&pubkey_str) {
            handle_sol_balance_update(store, &pubkey_str, account.lamports).await?;
        }

        // Check if this is a token account update
        if account.owner == spl_token::ID.to_bytes().to_vec() {
            if let Ok(token_account) = TokenAccount::unpack(&account.data) {
                let owner_pubkey_str = bs58::encode(&token_account.owner).into_string();
                if monitored_addresses.contains(&owner_pubkey_str) {
                    handle_token_balance_update(store, &owner_pubkey_str, token_account).await?;
                }
            }
        }
    }
    Ok(())
}

async fn handle_sol_balance_update(
    store: &Store,
    pubkey: &str,
    lamports: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let user = match store.get_user_by_public_key(pubkey).await? {
        Some(u) => u,
        None => {
            error!("SOL balance update for a public key not associated with any user: {}", pubkey);
            return Ok(());
        }
    };

    let sol_asset = store
        .upsert_asset("So11111111111111111111111111111111111111112", 9, "Solana", "SOL")
        .await?;

    store
        .upsert_balance(user.id, sol_asset.id, lamports as i64)
        .await?;

    info!("Updated SOL balance for {}: {} SOL", pubkey, lamports as f64 / 1e9);

    Ok(())
}

async fn handle_token_balance_update(
    store: &Store,
    owner_pubkey: &str,
    token_account: TokenAccount,
) -> Result<(), Box<dyn std::error::Error>> {
    let user = match store.get_user_by_public_key(owner_pubkey).await? {
        Some(u) => u,
        None => {
             error!("Token balance update for a public key not associated with any user: {}", owner_pubkey);
            return Ok(());
        }
    };

    let mint_address = bs58::encode(&token_account.mint).into_string();
    let (name, symbol) = get_token_metadata(&mint_address);

    let asset = store
        .upsert_asset(&mint_address, token_account.mint.get_decimals()? as i32, &name, &symbol)
        .await?;

    store
        .upsert_balance(user.id, asset.id, token_account.amount as i64)
        .await?;

    info!(
        "Updated token balance for {} [{}]: {}",
        owner_pubkey, symbol, token_account.amount
    );

    Ok(())
}

fn get_token_metadata(mint_address: &str) -> (String, String) {
    let mut known_tokens = HashMap::new();
    known_tokens.insert(
        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", // USDC
        ("USD Coin".to_string(), "USDC".to_string()),
    );
    known_tokens.insert(
        "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB", // USDT
        ("Tether".to_string(), "USDT".to_string()),
    );

    if let Some(metadata) = known_tokens.get(mint_address) {
        metadata.clone()
    } else {
        (format!("Unknown Token"), format!("UNKNOWN-{}", &mint_address[..4]))
    }
}
