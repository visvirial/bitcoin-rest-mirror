
use std::time::{
    Duration,
};
use std::sync::{
    Arc,
    RwLock,
};
use std::thread::{
    sleep,
    available_parallelism,
};
use std::collections::HashMap;
use num_format::{
    Locale,
    ToFormattedString,
};

use bitcoin_rest_mirror::{
    load_config,
    blk_reader::BlkReader,
    client::{
        RedisClientPool,
        Client,
    },
};

use bitcoin_rest_mirror::block_downloader::BlockDownloader;

async fn sync_single(downloader: &mut BlockDownloader, client: &Client) -> u32 {
    let next_block_height = client.get_next_block_height();
    downloader.run(next_block_height).await.unwrap();
    let mut blocks_processed = 0;
    loop {
        let block = downloader.shift().await;
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

async fn sync_multi(downloader: &mut BlockDownloader, client: &Client) -> u32 {
    let next_block_height = client.get_next_block_height();
    downloader.run(next_block_height).await.unwrap();
    let mut blocks_processed = 0;
    let processed_blocks = Arc::new(RwLock::new(HashMap::<u32, bool>::new()));
    // Launch threads.
    let concurrency = available_parallelism().unwrap().get();
    let (tx, mut rx) = tokio::sync::mpsc::channel(5 * concurrency);
    println!("Starting {} threads...", concurrency);
    for _ in 0..concurrency {
        let mut downloader = downloader.clone();
        let client = client.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            loop {
                let block = downloader.shift().await;
                if block.is_none() {
                    //println!("Processed all blocks.");
                    tx.send((downloader.get_current_height(), false)).await.unwrap();
                    break;
                }
                let (height, block) = block.unwrap();
                client.add_block(height, block.into(), Some(false));
                tx.send((height, true)).await.unwrap();
            }
        });
    }
    // Receive processed block heights.
    let rc_thread = {
        let processed_blocks = processed_blocks.clone();
        tokio::spawn(async move {
            loop {
                let (height, block_exists) = rx.recv().await.unwrap();
                processed_blocks.write().unwrap().insert(height, block_exists);
            }
        })
    };
    // Wait for next block to be processed.
    for height in next_block_height.. {
        let processed_new_block = loop {
            match processed_blocks.read().unwrap().get(&height) {
                Some(true) => {
                    client.set_next_block_height(height + 1);
                    blocks_processed += 1;
                    break true;
                },
                Some(false) => {
                    break false;
                },
                None => {
                },
            };
            sleep(Duration::from_millis(100));
        };
        if !processed_new_block {
            break;
        }
    }
    rc_thread.abort();
    blocks_processed
}

async fn sync_initial(blk_reader: &mut BlkReader, client: &Client) -> u32 {
    blk_reader.run_threads(4).await;
    let mut blocks_processed = 0;
    let processed_blocks = Arc::new(RwLock::new(HashMap::<u32, bool>::new()));
    // Launch threads.
    let concurrency = available_parallelism().unwrap().get();
    let (tx, mut rx) = tokio::sync::mpsc::channel(5 * concurrency);
    println!("Starting {} threads...", concurrency);
    for _ in 0..concurrency {
        let mut blk_reader = blk_reader.clone();
        let client = client.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            loop {
                let block_data = blk_reader.get_next_block().await;
                if block_data.is_none() {
                    //println!("Processed all blocks.");
                    tx.send((blk_reader.get_next_height(), false)).await.unwrap();
                    break;
                }
                let (height, block) = block_data.unwrap();
                client.add_block(height, block.into(), Some(false));
                tx.send((height, true)).await.unwrap();
            }
        });
    }
    // Receive processed block heights.
    let rc_thread = {
        let processed_blocks = processed_blocks.clone();
        tokio::spawn(async move {
            loop {
                let (height, block_exists) = rx.recv().await.unwrap();
                processed_blocks.write().unwrap().insert(height, block_exists);
            }
        })
    };
    // Wait for next block to be processed.
    for height in 0.. {
        let processed_new_block = loop {
            match processed_blocks.read().unwrap().get(&height) {
                Some(true) => {
                    client.set_next_block_height(height + 1);
                    blocks_processed += 1;
                    break true;
                },
                Some(false) => {
                    break false;
                },
                None => {
                },
            };
            sleep(Duration::from_millis(100));
        };
        if !processed_new_block {
            break;
        }
    }
    rc_thread.abort();
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
    // Initialize blk_reader.
    let blocks_dir = chain_config["blocksDir"].as_str().expect("blocksDir not set").to_string();
    println!("Reading blocks from: {}", blocks_dir);
    let mut blk_reader = BlkReader::new(blocks_dir);
    // Initialize block downloader.
    let concurrency = config["downloader"]["concurrency"].as_i64().unwrap_or(4) as usize;
    let mut downloader = BlockDownloader::new(Some(chain_config["restUrl"].as_str().expect("restUrl not set").to_string()))
        .set_concurrency(concurrency)
        ;
    // Fetch next block height.
    let next_block_height = client.get_next_block_height();
    // Print stats.
    let reporter_thread = {
        let client = client.clone();
        let reporter_thread = tokio::spawn(async move {
            let mut last_block_height: i32 = next_block_height as i32 - 1;
            loop {
                tokio::time::sleep(Duration::from_millis(1000)).await;
                let current_height = client.get_next_block_height() as i32;
                let processed_blocks = current_height - last_block_height;
                println!(
                    "Processing: #{}, Blocks per second: {}",
                    current_height.to_formatted_string(&Locale::en),
                    processed_blocks.to_formatted_string(&Locale::en),
                );
                last_block_height = current_height;
            }
        });
        reporter_thread
    };
    // Do initial sync.
    let blocks_processed = if next_block_height == 0 {
        println!("Starting initial sync...");
        sync_initial(&mut blk_reader, &client).await
    } else {
        println!("Starting multi-threaded sync...");
        sync_multi(&mut downloader, &client).await
    };
    // Stop reporter thread.
    reporter_thread.abort();
    println!(
        "First sync completed: synced {} blocks.",
        blocks_processed.to_formatted_string(&Locale::en),
    );
    // Start sync loop.
    loop {
        sleep(Duration::from_millis(1000));
        let blocks_processed = sync_single(&mut downloader, &client).await;
        if blocks_processed == 0 {
            continue;
        }
        println!(
            "Synced {} blocks.",
            blocks_processed.to_formatted_string(&Locale::en),
        );
    }
}

