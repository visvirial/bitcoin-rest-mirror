
use std::fs::File;
use std::io::prelude::*;
use yaml_rust2::{
    Yaml,
    YamlLoader
};
use axum::{
    Router,
    routing::get,
    extract::{
        Path,
        State,
    },
    http::StatusCode,
    response::{
        Response,
        IntoResponse,
    },
};

mod client;
use client::{
    RedisClientPool,
    Client,
};

pub fn load_config() -> Yaml {
    let mut f = File::open("../config.yaml").expect("config.yaml file not found");
    let mut contents = String::new();
    f.read_to_string(&mut contents).expect("Something went wrong reading the file");
    let config = YamlLoader::load_from_str(&contents).unwrap();
    config[0].clone()
}

/*
 * @return (hash, ext)
 */
pub fn parse_id_and_ext(path: &str) -> Result<([u8; 32], String), &'static str> {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.len() != 2 {
        return Err("Invalid path");
    }
    let hash = match hex::decode(parts[0]) {
        Ok(mut hash) => {
            if hash.len() != 32 {
                return Err("Invalid hash length");
            }
            let mut hash_array = [0u8; 32];
            hash.reverse();
            hash_array.copy_from_slice(&hash);
            hash_array
        },
        Err(_) => return Err("Invalid hash")
    };
    Ok((hash, parts[1].to_string()))
}

fn make_response(data: Vec<u8>, ext: &str) -> Response {
    if ext == "hex" {
        return (StatusCode::OK, hex::encode(data)).into_response();
    } else if ext == "bin" {
        return (StatusCode::OK, data).into_response();
    } else {
        return (StatusCode::BAD_REQUEST, "Invalid extension".to_string()).into_response();
    }
}

async fn handle_tx(State(mut state): State<AppState>, Path(path): Path<String>) -> impl IntoResponse {
    let (hash, ext) = match parse_id_and_ext(&path) {
        Ok((hash, ext)) => (hash, ext),
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    let tx = match state.client.get_transaction(&hash) {
        Some(tx) => tx,
        None => return (StatusCode::NOT_FOUND, "Transaction not found".to_string()).into_response(),
    };
    make_response(tx, ext.as_str())
}

async fn handle_block(State(mut state): State<AppState>, Path(path): Path<String>) -> impl IntoResponse {
    let (hash, ext) = match parse_id_and_ext(&path) {
        Ok((hash, ext)) => (hash, ext),
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    let block = match state.client.get_block(&hash) {
        Some(block) => block,
        None => return (StatusCode::NOT_FOUND, "Block not found".to_string()).into_response(),
    };
    make_response(block, ext.as_str())
}

#[derive(Clone)]
pub struct AppState {
    client: Client,
}

pub async fn start_server(client: Client, port: u16, host: &str) {
    let app_state = AppState {
        client,
    };
    let app = Router::new()
        .route("/rest/tx/:tx_hash", get(handle_tx))
        .route("/rest/block/:block_hash", get(handle_block))
        .with_state(app_state)
        ;
    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(addr.clone()).await.unwrap();
    println!("HTTP server listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}

#[tokio::main]
async fn main() {
    // Load chain.
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!("Usage: {} <chain>", args[0]);
        std::process::exit(1);
    }
    let chain = &args[1];
    // Load config.
    let config = load_config();
    // Initialize Redis connection.
    let redis_url = config["redisUrl"].as_str().unwrap();
    // Initialize client.
    let redis_client = RedisClientPool::new(redis_url, chain.clone(), None);
    let client = Client::new(redis_client);
    // Initialize server.
    let port = config["chains"][chain.as_str()]["server"]["port"].as_i64().unwrap_or(8000);
    let host = config["chains"][chain.as_str()]["server"]["host"].as_str().unwrap_or("localhost");
    start_server(client, port as u16, host).await;
}

/*
#[cfg(test)]
mod tests {
    use super::*;
    use redis_test::MockRedisConnection;
    fn load_block(height: u32) -> Vec<u8> {
        let mut f = File::open(format!("../test/fixtures/block_{}.bin", height)).expect("block file not found");
        let mut block = Vec::new();
        f.read_to_end(&mut block).expect("Something went wrong reading the file");
        block
    }
    fn prepare_redis() {
        let redis_connection = MockRedisConnection::new(vec![]);
    }
    #[test]
    fn test() {
        assert_eq!(1, 1);
    }
}
*/

