
use std::fs::File;
use std::io::prelude::*;
use yaml_rust2::{
    Yaml,
    YamlLoader
};
use bitcoin_hashes::Sha256d;

pub mod blk_reader;
pub mod block_downloader;
pub mod client;
pub mod server;

pub type Binary = Vec<u8>;

pub fn load_config() -> Yaml {
    let mut f = File::open("./config.yaml").expect("config.yaml file not found");
    let mut contents = String::new();
    f.read_to_string(&mut contents).expect("Something went wrong reading the file");
    let config = YamlLoader::load_from_str(&contents).unwrap();
    config[0].clone()
}

pub fn block_to_block_hash(block: &[u8]) -> [u8; 32] {
    if block.len() < 80 {
        panic!("Block is too short.");
    }
    Sha256d::hash(&block[0..80]).to_byte_array()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    mod block_to_block_hash {
        use super::*;
        #[test]
        fn header() {
            let block = hex::decode("0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c").unwrap();
            let mut block_hash = block_to_block_hash(&block);
            block_hash.reverse();
            assert_eq!(hex::encode(block_hash), "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f");
        }
        #[test]
        fn block() {
            let block = hex::decode("0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c0101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000").unwrap();
            let mut block_hash = block_to_block_hash(&block);
            block_hash.reverse();
            assert_eq!(hex::encode(block_hash), "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f");
        }
    }
    
}

