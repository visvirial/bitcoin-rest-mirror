
use std::fs::File;
use std::io::prelude::*;
use std::sync::{
    Arc,
    Mutex,
};
use yaml_rust2::{
    Yaml,
    YamlLoader
};
use redis::Commands;
use axum::{
    Router,
    routing::get,
    extract::{
        Path,
        State,
    },
    http::StatusCode,
    response::IntoResponse,
};

pub struct Client {
    redis_conn: redis::Connection,
    chain: String,
    prefix: String,
}

impl Client {
    pub fn new(redis_conn: redis::Connection, chain: String, prefix: Option<String>) -> Self {
        Self {
            redis_conn,
            chain,
            prefix: prefix.unwrap_or("bitcoin-rest-mirror".to_string())
        }
    }
    pub fn get(&mut self, prefix: &str, key: &str) -> Option<Vec<u8>> {
        let key = format!("{}:{}:{}:{}", self.prefix, self.chain, prefix, key);
        let value: Option<String> = self.redis_conn.get(key).unwrap();
        match value {
            Some(value) => Some(value.into_bytes()),
            None => None
        }
    }
    pub fn get_transaction(&mut self, tx_hash: &[u8; 32]) -> Option<Vec<u8>> {
        self.get("transaction", hex::encode(tx_hash).as_str())
    }
}

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

async fn handle_tx(State(state): State<Arc<Mutex<AppState>>>, Path(path): Path<String>) -> impl IntoResponse {
    let (hash, ext) = match parse_id_and_ext(&path) {
        Ok((hash, ext)) => (hash, ext),
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    let tx = match state.lock().unwrap().client.get_transaction(&hash) {
        Some(tx) => tx,
        None => return (StatusCode::NOT_FOUND, "Transaction not found".to_string()).into_response(),
    };
    if ext == "hex" {
        return (StatusCode::OK, hex::encode(tx)).into_response();
    } else if ext == "bin" {
        return (StatusCode::OK, tx).into_response();
    } else {
        return (StatusCode::BAD_REQUEST, "Invalid extension".to_string()).into_response();
    }
}

pub struct AppState {
    client: Client,
}

pub async fn start_server(client: Client, port: u16, host: &str) {
    /*
    let client = &mut self.client;
    let handle_tx_hex = |Path(path): Path<String>| async move {
    };
    */
    let app = Router::new()
        .route("/rest/tx/:tx_hash.hex", get(handle_tx))
        .with_state(Arc::new(Mutex::new(AppState { client  })))
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
    let redis = redis::Client::open(redis_url).unwrap();
    let conn = redis.get_connection().expect("Failed to connect to Redis server");
    // Initialize client.
    let client = Client::new(conn, chain.clone(), None);
    // Initialize server.
    let port = config["chains"][chain.as_str()]["server"]["port"].as_i64().unwrap_or(8000);
    let host = config["chains"][chain.as_str()]["server"]["host"].as_str().unwrap_or("localhost");
    start_server(client, port as u16, host).await;
}

