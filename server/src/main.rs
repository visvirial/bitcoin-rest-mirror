
use std::fs::File;
use std::io::prelude::*;
use std::sync::{
    Arc,
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
    response::{
        Response,
        IntoResponse,
    },
};
use bitcoin::consensus::Encodable;

#[derive(Clone)]
pub struct Client {
    pool: Arc<r2d2::Pool<redis::Client>>,
    chain: String,
    prefix: String,
}

impl Client {
    pub fn new(redis_url: &str, chain: String, prefix: Option<String>) -> Self {
        let client = redis::Client::open(redis_url).unwrap();
        let pool = Arc::new(r2d2::Pool::builder().build(client).unwrap());
        Self {
            pool,
            chain,
            prefix: prefix.unwrap_or("bitcoin-rest-mirror".to_string())
        }
    }
    pub fn get(&mut self, prefix: &str, key: &str) -> Option<Vec<u8>> {
        let key = format!("{}:{}:{}:{}", self.prefix, self.chain, prefix, key);
        let value: Option<Vec<u8>> = self.pool.get().unwrap().get(&key).unwrap();
        value
    }
    pub fn get_transaction(&mut self, tx_hash: &[u8; 32]) -> Option<Vec<u8>> {
        self.get("transaction", hex::encode(tx_hash).as_str())
    }
    pub fn get_block_header(&mut self, block_hash: &[u8; 32]) -> Option<[u8; 80]> {
        let block_header = self.get("blockHeader", hex::encode(block_hash).as_str());
        match block_header {
            Some(block_header) => {
                let mut block_header_array = [0u8; 80];
                block_header_array.copy_from_slice(&block_header);
                Some(block_header_array)
            },
            None => None
        }
    }
    pub fn get_block_transaction_hashes(&mut self, block_hash: &[u8; 32]) -> Option<Vec<[u8; 32]>> {
        let tx_hashes = self.get("blockTransactionHashes", hex::encode(block_hash).as_str());
        match tx_hashes {
            Some(tx_hashes) => {
                let mut tx_hashes_array = Vec::new();
                for i in 0..(tx_hashes.len() / 32) {
                    let mut tx_hash = [0u8; 32];
                    tx_hash.copy_from_slice(&tx_hashes[(i * 32)..((i + 1) * 32)]);
                    tx_hashes_array.push(tx_hash);
                }
                Some(tx_hashes_array)
            },
            None => None
        }
    }
    pub fn get_block(&mut self, block_hash: &[u8; 32]) -> Option<Vec<u8>> {
        let block_header = match self.get_block_header(block_hash) {
            Some(block_header) => block_header,
            None => return None
        };
        let tx_hashes = match self.get_block_transaction_hashes(block_hash) {
            Some(tx_hashes) => tx_hashes,
            None => return None
        };
        let txs = tx_hashes.iter().map(|tx_hash| self.get_transaction(tx_hash)).collect::<Vec<Option<Vec<u8>>>>();
        if txs.iter().any(|tx| tx.is_none()) {
            return None;
        }
        let txs = txs.into_iter().flatten().collect::<Vec<Vec<u8>>>();
        let tx_length_varint = bitcoin::VarInt::from(txs.len());
        let mut tx_length_vec = Vec::new();
        tx_length_varint.consensus_encode(&mut tx_length_vec).unwrap();
        let mut block = Vec::new();
        block.extend(block_header);
        block.extend(tx_length_vec);
        block.extend(txs.iter().flatten());
        Some(block)
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
    let app = Router::new()
        .route("/rest/tx/:tx_hash", get(handle_tx))
        .route("/rest/block/:block_hash", get(handle_block))
        .with_state(AppState { client  })
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
    let client = Client::new(redis_url, chain.clone(), None);
    // Initialize server.
    let port = config["chains"][chain.as_str()]["server"]["port"].as_i64().unwrap_or(8000);
    let host = config["chains"][chain.as_str()]["server"]["host"].as_str().unwrap_or("localhost");
    start_server(client, port as u16, host).await;
}

#[cfg(test)]
mod tests {
}

