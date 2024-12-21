
use std::time::SystemTime;
use num_format::{
    Locale,
    ToFormattedString,
};

use bitcoin_rest_block_downloader::BlockDownloader;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rest_endpoint = if args.len() >= 2 {
        Some(args[1].clone())
    } else {
        None
    };
    let mut downloader = BlockDownloader::new(rest_endpoint)
        .set_concurrency(4)
        ;
    downloader.run(0).await.unwrap();
    println!("Downloader started.");
    let mut lap_time = SystemTime::now();
    let mut fetched_blocks: usize = 0;
    loop {
        let block = downloader.shift();
        if block.is_none() {
            println!("No more blocks to fetch.");
            break;
        }
        fetched_blocks += 1;
        let elapsed = lap_time.elapsed().unwrap().as_millis();
        if elapsed >= 1000 {
            println!("Blocks per second: {}.", (fetched_blocks * 1000 / elapsed as usize).to_formatted_string(&Locale::en));
            lap_time = SystemTime::now();
            fetched_blocks = 0;
        }
    }
}

