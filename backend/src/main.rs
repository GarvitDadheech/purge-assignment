use actix_web::{web, App, HttpServer};
use dotenv::dotenv;
use sqlx::PgPool;
use std::env;
use store::Store;

mod routes;
mod middleware;

use routes::*;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::init();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to create pool.");
    let store = Store::new(pool);
    let store_data = web::Data::new(store);

    HttpServer::new(move || {
        App::new()
            .app_data(store_data.clone())
            .service(
                web::scope("/api/v1")
                    .service(sign_up)
                    .service(sign_in)
                    .service(get_user)
                    .service(quote)
                    .service(swap)
                    .service(send)
                    .service(sol_balance)
                    .service(token_balance),
            )
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
