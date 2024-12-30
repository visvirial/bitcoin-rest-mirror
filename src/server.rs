
use axum::{
    Router,
    routing::get,
    extract::{
        Path,
        Query,
        State,
    },
    http::StatusCode,
    response::{
        Response,
        IntoResponse,
    },
};
use serde::Deserialize;

use crate::client::Client;

/*
 * @return (hash, ext)
 */
fn parse_id_and_ext(path: &str) -> Result<([u8; 32], String), &'static str> {
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

fn parse_number_and_ext(path: &str) -> Result<(u32, String), &'static str> {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.len() != 2 {
        return Err("Invalid path");
    }
    let num = match parts[0].parse::<u32>() {
        Ok(num) => {
            num
        },
        Err(_) => return Err("Invalid height")
    };
    Ok((num, parts[1].to_string()))
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

async fn handle_tx(state: State<AppState>, path: Path<String>) -> impl IntoResponse {
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

async fn handle_block(state: State<AppState>, path: Path<String>) -> impl IntoResponse {
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

#[derive(Deserialize)]
struct HeadersQuery {
    count: Option<usize>,
}

async fn handle_headers(state: State<AppState>, path: Path<String>, query: Query<HeadersQuery>) -> impl IntoResponse {
    let (hash, ext) = match parse_id_and_ext(&path) {
        Ok((hash, ext)) => (hash, ext),
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    let count = query.count.unwrap_or(5);
    // Get height of the block hash.
    let height = match state.client.get_block_height_by_hash(&hash) {
        Some(height) => height,
        None => return (StatusCode::NOT_FOUND, "Block not found".to_string()).into_response(),
    };
    let mut block_headers = Vec::new();
    for i in 0..count {
        // Get block hash.
        match state.client.get_block_hash_by_height(height + i as u32) {
            Some(block_hash) => {
                match state.client.get_block_header(&block_hash) {
                    Some(block_header) => {
                        block_headers.push(block_header);
                    },
                    None => return (StatusCode::INTERNAL_SERVER_ERROR, "Block header not found".to_string()).into_response(),
                };
            },
            None => break,
        };
    }
    // Concatenate block headers.
    let block_headers = block_headers.concat();
    make_response(block_headers, ext.as_str())
}

async fn handle_blockhashbyheight(state: State<AppState>, path: Path<String>) -> impl IntoResponse {
    let (height, ext) = match parse_number_and_ext(&path) {
        Ok((height, ext)) => (height, ext),
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    let mut block_hash = match state.client.get_block_hash_by_height(height) {
        Some(block_hash) => block_hash,
        None => return (StatusCode::NOT_FOUND, "Block not found".to_string()).into_response(),
    };
    if ext == "hex" {
        block_hash.reverse();
    }
    make_response(block_hash.to_vec(), ext.as_str())
}

#[derive(Clone)]
struct AppState {
    client: Client,
}

fn create_app(client: Client) -> Router {
    let app_state = AppState {
        client,
    };
    let app = Router::new()
        .route("/rest/tx/:tx_hash", get(handle_tx))
        .route("/rest/block/:block_hash", get(handle_block))
        .route("/rest/headers/:block_hash", get(handle_headers))
        .route("/rest/blockhashbyheight/:height", get(handle_blockhashbyheight))
        .with_state(app_state);
    app
}

pub async fn start_server(client: Client, port: u16, host: &str) {
    let app = create_app(client);
    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(addr.clone()).await.unwrap();
    println!("HTTP server listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    
    use super::*;
    
    use axum_test::TestServer;
    use bitcoin::block::Block;
    use bitcoin::consensus::{
        Decodable,
        Encodable,
    };
    
    #[tokio::test]
    async fn test() {
        // Initialize client.
        let client = crate::client::tests::create_client();
        // Load blocks.
        let blocks = crate::client::tests::load_blocks();
        // Register blocks.
        for height in 0..blocks.len() {
            let block = &blocks[height];
            client.add_block(height as u32, block.clone(), None);
        }
        let app = create_app(client);
        let server = TestServer::new(app).unwrap();
        for height in 0..blocks.len() {
            let block = Block::consensus_decode(&mut blocks[height].as_slice()).unwrap();
            let mut block_header = [0u8; 80];
            block.header.consensus_encode(&mut block_header.as_mut()).unwrap();
            let block_hash: [u8; 32] = *block.block_hash().as_ref();
            let mut block_id = block_hash.clone();
            block_id.reverse();
            let block_hash_response = server.get(format!("/rest/blockhashbyheight/{}.hex", height).as_str())
                .await
                .text();
            assert_eq!(block_hash_response, hex::encode(&block_id));
            let block_headers_response = server.get(format!("/rest/headers/{}.hex?count=1", hex::encode(block_id)).as_str())
                .await
                .text();
            assert_eq!(block_headers_response, hex::encode(&block_header));
            for tx in block.txdata {
                let mut txid: [u8; 32] = *tx.compute_txid().as_ref();
                txid.reverse();
                let tx_response = server.get(format!("/rest/tx/{}.hex", hex::encode(txid)).as_str())
                    .await
                    .text();
                let mut tx_vec = Vec::new();
                tx.consensus_encode(&mut tx_vec).unwrap();
                assert_eq!(tx_response, hex::encode(tx_vec));
            }
            let block_response = server.get(format!("/rest/block/{}.hex", hex::encode(block_id)).as_str())
                .await
                .text();
            assert_eq!(block_response, hex::encode(&blocks[height]));
        }
    }
    
}

