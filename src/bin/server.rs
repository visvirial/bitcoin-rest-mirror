
use bitcoin_rest_mirror::{
    load_config,
    client::{
        RedisClientPool,
        Client,
    },
    server::start_server,
};

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
    let chain_config = &config["chains"][chain.as_str()];
    // Initialize Redis connection.
    let redis_url = config["redisUrl"].as_str().unwrap();
    // Initialize client.
    let redis_client = RedisClientPool::new(redis_url);
    let client = Client::new(redis_client, chain.clone(), None);
    // Initialize server.
    let port = chain_config["server"]["port"].as_i64().unwrap_or(8000);
    let host = chain_config["server"]["host"].as_str().unwrap_or("localhost");
    start_server(client, port as u16, host).await;
}

