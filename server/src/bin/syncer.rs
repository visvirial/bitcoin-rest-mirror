
use std::time::SystemTime;
use num_format::{
    Locale,
    ToFormattedString,
};

use bitcoin_rest_mirror::{
    load_config,
    client::{
        RedisClientPool,
        Client,
    },
};

use bitcoin_rest_block_downloader::BlockDownloader;

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
    // Initialize block downloader.
    let concurrency = config["downloader"]["concurrency"].as_i64().unwrap_or(4) as usize;
    let mut downloader = BlockDownloader::new(Some(chain_config["restUrl"].as_str().expect("restUrl not set").to_string()))
        .set_concurrency(concurrency)
        ;
    // Fetch next block height.
    let next_block_height = client.get_next_block_height();
    // Start downloader.
    downloader.run(next_block_height).await.unwrap();
    println!("Downloader started.");
    // Process blocks.
    let mut lap_time = SystemTime::now();
    let mut fetched_blocks: usize = 0;
    loop {
        let block = downloader.shift();
        if block.is_none() {
            println!("Processed all blocks.");
            break;
        }
        let (height, block) = block.unwrap();
        client.add_block(height, block.into(), Some(true));
        fetched_blocks += 1;
        let elapsed = lap_time.elapsed().unwrap().as_millis();
        if elapsed >= 1000 {
            println!(
                "Processing: #{}, Blocks per second: {}, Blocks waiting: {}.",
                downloader.get_current_height().to_formatted_string(&Locale::en),
                (fetched_blocks * 1000 / elapsed as usize).to_formatted_string(&Locale::en),
                downloader.get_blocks_count().to_formatted_string(&Locale::en),
            );
            lap_time = SystemTime::now();
            fetched_blocks = 0;
        }
    }
}

