
use std::time::SystemTime;
use num_format::{
    Locale,
    ToFormattedString,
};

use bitcoin_rest_mirror::block_downloader::BlockDownloader;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rest_endpoint = if args.len() >= 2 {
        Some(args[1].clone())
    } else {
        None
    };
    let concurrency = if args.len() >= 3 {
        args[2].parse::<usize>().unwrap()
    } else {
        4
    };
    let mut downloader = BlockDownloader::new(rest_endpoint)
        .set_concurrency(concurrency)
        ;
    downloader.run(0).await.unwrap();
    println!("Downloader started.");
    let mut lap_time = SystemTime::now();
    let mut fetched_blocks: usize = 0;
    loop {
        let block = downloader.shift().await;
        if block.is_none() {
            println!("No more blocks to fetch.");
            break;
        }
        fetched_blocks += 1;
        let elapsed = lap_time.elapsed().unwrap().as_millis();
        if elapsed >= 1000 {
            println!(
                "Processing: #{}, Blocks per second: {}.",
                downloader.get_current_height().to_formatted_string(&Locale::en),
                (fetched_blocks * 1000 / elapsed as usize).to_formatted_string(&Locale::en)
            );
            lap_time = SystemTime::now();
            fetched_blocks = 0;
        }
    }
}

