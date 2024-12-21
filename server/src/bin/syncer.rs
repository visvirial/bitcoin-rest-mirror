
use std::time::{
    Duration,
};
use std::thread::{
    sleep,
};
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

async fn try_sync(downloader: &mut BlockDownloader, client: &Client) -> u32 {
    let next_block_height = client.get_next_block_height();
    downloader.run(next_block_height).await.unwrap();
    let mut blocks_processed = 0;
    loop {
        let block = downloader.shift();
        if block.is_none() {
            //println!("Processed all blocks.");
            break;
        }
        let (height, block) = block.unwrap();
        client.add_block(height, block.into(), Some(true));
        blocks_processed += 1;
    }
    blocks_processed
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
    // Print stats.
    let reporter_thread = {
        let downloader = downloader.clone();
        let reporter_thread = tokio::spawn(async move {
            let mut last_block_height: i32 = next_block_height as i32 - 1;
            loop {
                tokio::time::sleep(Duration::from_millis(1000)).await;
                let current_height = downloader.get_current_height() as i32;
                let processed_blocks = current_height - last_block_height;
                println!(
                    "Processing: #{}, Blocks per second: {}, Blocks waiting: {}.",
                    current_height.to_formatted_string(&Locale::en),
                    processed_blocks.to_formatted_string(&Locale::en),
                    downloader.get_blocks_count().to_formatted_string(&Locale::en),
                );
                last_block_height = current_height;
            }
        });
        reporter_thread
    };
    // Do initial sync.
    println!("Starting initial sync...");
    let blocks_processed = try_sync(&mut downloader, &client).await;
    println!(
        "Initial sync completed: synced {} blocks.",
        blocks_processed.to_formatted_string(&Locale::en),
    );
    // Stop reporter thread.
    reporter_thread.abort();
    // Start sync loop.
    loop {
        sleep(Duration::from_millis(1000));
        let blocks_processed = try_sync(&mut downloader, &client).await;
        if blocks_processed == 0 {
            continue;
        }
        println!(
            "Synced {} blocks.",
            blocks_processed.to_formatted_string(&Locale::en),
        );
    }
}

