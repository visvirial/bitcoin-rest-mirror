
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

use bitcoin_rest_mirror::blk_reader::BlkReader;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!("Usage: {} <blocks_dir>", args[0]);
        std::process::exit(1);
    }
    let blocks_dir = &args[1];
    let mut blk_reader = BlkReader::new(blocks_dir.clone());
    println!("Reading blocks from: {}", blocks_dir);
    let concurrency = 4;
    blk_reader.run_threads(concurrency).await;
    let mut last_print = std::time::Instant::now();
    loop {
        if blk_reader.is_all_read() {
            break;
        }
        while let Some((height, block)) = blk_reader.get_next_block() {
            if last_print.elapsed().as_secs() >= 1 {
                let block_header = Header::consensus_decode(&mut block.as_ref()).unwrap();
                let mut block_hash: [u8; 32] = *block_header.block_hash().as_ref();
                block_hash.reverse();
                println!(
                    "Block height: #{}, Hash: {}, Block size: {}",
                    height.to_formatted_string(&Locale::en),
                    hex::encode(block_hash),
                    block.len().to_formatted_string(&Locale::en),
                );
                last_print = std::time::Instant::now();
            }
        }
    }
    println!("All blocks read.");
}

