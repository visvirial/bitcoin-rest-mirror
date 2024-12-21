
use std::fs::File;
use std::io::prelude::*;
use yaml_rust2::{
    Yaml,
    YamlLoader
};

use bitcoin_rest_mirror::client::{
    RedisClientPool,
    Client,
};

use bitcoin_rest_mirror::server::start_server;

pub fn load_config() -> Yaml {
    let mut f = File::open("../config.yaml").expect("config.yaml file not found");
    let mut contents = String::new();
    f.read_to_string(&mut contents).expect("Something went wrong reading the file");
    let config = YamlLoader::load_from_str(&contents).unwrap();
    config[0].clone()
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
    let redis_client = RedisClientPool::new(redis_url);
    let client = Client::new(redis_client, chain.clone(), None);
    // Initialize server.
    let port = config["chains"][chain.as_str()]["server"]["port"].as_i64().unwrap_or(8000);
    let host = config["chains"][chain.as_str()]["server"]["host"].as_str().unwrap_or("localhost");
    start_server(client, port as u16, host).await;
}

