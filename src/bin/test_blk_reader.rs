
use num_format::{
    Locale,
    ToFormattedString,
};
use bitcoin::block::{
    Header,
};
use bitcoin::consensus::{
    Decodable,
    //Encodable,
};

use bitcoin_rest_mirror::{
    load_config,
    blk_reader::BlkReader,
    block_downloader::BitcoinRest,
};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!("Usage: {} <chain>", args[0]);
        std::process::exit(1);
    }
    let chain = &args[1];
    // Load config.
    let config = load_config();
    let chain_config = &config["chains"][chain.as_str()];
    // Initialize BitcoinRest.
    let bitcoin_rest = BitcoinRest::new(Some(chain_config["restUrl"].as_str().unwrap().to_string()));
    // Initialize BlkReader.
    let blocks_dir = chain_config["blocksDir"].as_str().unwrap().to_string();
    let mut blk_reader = BlkReader::new(blocks_dir.clone());
    blk_reader.init(&bitcoin_rest, 0).await;
    // Start reading blocks.
    println!("Reading blocks from: {}", blocks_dir);
    let concurrency = 4;
    blk_reader.run_threads(concurrency).await;
    let mut last_print = std::time::Instant::now();
    while let Some((height, block_bytes)) = blk_reader.get_next_block().await {
        if last_print.elapsed().as_secs() >= 1 {
            let block_header = Header::consensus_decode(&mut block_bytes.as_ref()).unwrap();
            let mut block_hash: [u8; 32] = *block_header.block_hash().as_ref();
            block_hash.reverse();
            println!(
                "Block height: #{}, Hash: {}, Block size: {}",
                height.to_formatted_string(&Locale::en),
                hex::encode(block_hash),
                block_bytes.len().to_formatted_string(&Locale::en),
            );
            last_print = std::time::Instant::now();
        }
    }
    println!("All blocks read.");
}

