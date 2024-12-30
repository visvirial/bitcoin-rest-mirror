
use std::io::prelude::*;
use std::io::BufReader;
use std::time::SystemTime;
use std::fs::File;
use std::sync::{
    Arc,
    RwLock,
};
use std::collections::{
    HashMap,
};
use bytes::Bytes;
use tokio::time::{
    sleep,
    Duration,
};
use bitcoin::{
    block::Header,
    consensus::Decodable,
};
use bitcoin_hashes::Sha256d;

use crate::{
    block_downloader::BitcoinRest,
};

#[derive(Clone)]
pub struct BlkReaderData {
    // Block height -> block,
    blocks: HashMap<u32, Bytes>,
    block_height_by_hash: HashMap<[u8; 32], u32>,
    next_blk_index: u32,
    next_height: u32,
    all_read: bool,
}

impl BlkReaderData {
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
            block_height_by_hash: HashMap::new(),
            next_blk_index: 0,
            next_height: 0,
            all_read: false,
        }
    }
}

#[derive(Clone)]
pub struct BlkReader {
    blocks_dir: String,
    max_blocks: u32,
    data: Arc<RwLock<BlkReaderData>>,
}

impl BlkReader {
    pub fn new(blocks_dir: String) -> Self {
        Self {
            blocks_dir,
            max_blocks: 5000,
            data: Arc::new(RwLock::new(BlkReaderData::new())),
        }
    }
    pub async fn init(&self, bitcoin_rest: &BitcoinRest, starting_height: u32) {
        // Get starting block hash.
        let start_block_hash = bitcoin_rest.get_blockhashbyheight(starting_height).await.unwrap();
        println!("Starting block hash: {}", hex::encode(start_block_hash));
        // Download all block headers.
        println!("Fetching all block headers...");
        let start_time = SystemTime::now();
        let headers = bitcoin_rest.get_all_headers(start_block_hash, None).await.unwrap();
        let blocks_len = headers.len();
        println!("Fetched {} block headers in {}ms.", blocks_len, start_time.elapsed().unwrap().as_millis());
        // Convert to block_height_by_hash.
        for (offset, header) in headers.iter().enumerate() {
            let block_hash = Sha256d::hash(header).to_byte_array();
            let height = starting_height + offset as u32;
            self.data.write().unwrap().block_height_by_hash.insert(block_hash, height);
        }
        self.data.write().unwrap().next_height = starting_height;
    }
    pub fn is_all_read(&self) -> bool {
        self.data.read().unwrap().all_read
    }
    pub fn set_max_blocks(mut self, max_blocks: u32) -> Self {
        self.max_blocks = max_blocks;
        self
    }
    pub fn get_registered_block_count(&self) -> usize {
        self.data.read().unwrap().blocks.len()
    }
    pub fn get_next_height(&self) -> u32 {
        self.data.read().unwrap().next_height
    }
    fn read_file(&mut self, index: u32) -> Result<u32, ()> {
        let path = format!("{}/blk{:05}.dat", self.blocks_dir, index);
        //println!("Reading: {}", path);
        let file = File::open(&path);
        if file.is_err() {
            return Err(());
        }
        let mut block_reader = BufReader::new(file.unwrap());
        let mut block_count = 0;
        loop {
            // Read magic bytes.
            let mut magic = [0u8; 4];
            if block_reader.read_exact(&mut magic).is_err() {
                return Ok(block_count);
            }
            //println!("Magic bytes: {}", hex::encode(magic));
            // Read block size.
            let mut size = [0u8; 4];
            if block_reader.read_exact(&mut size).is_err() {
                return Ok(block_count);
            }
            let size = u32::from_le_bytes(size);
            //println!("Block size: {}", size);
            // Read block.
            let mut block_vec = vec![0u8; size as usize];
            if block_reader.read_exact(&mut block_vec).is_err() {
                return Ok(block_count);
            }
            block_count += 1;
            // Compute block hash.
            let block_header = Header::consensus_decode::<&[u8]>(&mut block_vec.as_ref());
            if block_header.is_err() {
                //println!("Failed to decode block header.");
                continue;
            }
            let block_header = block_header.unwrap();
            let block_hash: [u8; 32] = *block_header.block_hash().as_ref();
            let block_height = self.data.read().unwrap().block_height_by_hash.get(&block_hash).cloned();
            if block_height.is_none() {
                //println!("Block height not found for hash: {}", hex::encode(block_hash));
                continue;
            }
            let block_height = block_height.unwrap();
            // Save blcok.
            self.data.write().unwrap().blocks.insert(block_height, Bytes::from(block_vec));
        }
    }
    pub fn read_next_file(&mut self) -> Result<u32, ()> {
        let next_blk_index = {
            let mut data = self.data.write().unwrap();
            let next_blk_index = data.next_blk_index;
            data.next_blk_index += 1;
            next_blk_index
        };
        let block_count = self.read_file(next_blk_index);
        if block_count.is_err() {
            return Err(());
        }
        Ok(block_count.unwrap())
    }
    pub async fn run_threads(&mut self, concurrency: usize) {
        let mut handles = Vec::new();
        for _ in 0..concurrency {
            let mut this = self.clone();
            let handle = tokio::spawn(async move {
                loop {
                    if this.get_registered_block_count() >= this.max_blocks as usize {
                        //println!("Max blocks reached.");
                        sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                    let result = this.read_next_file();
                    if result.is_err() {
                        break;
                    }
                }
            });
            handles.push(handle);
        }
        {
            let this = self.clone();
            tokio::spawn(async move {
                futures::future::join_all(handles).await;
                this.data.write().unwrap().all_read = true;
            });
        }
    }
    pub fn try_get_next_block(&mut self) -> Option<(u32, Bytes)> {
        let mut data = self.data.write().unwrap();
        let next_height = data.next_height;
        if let Some(block) = data.blocks.remove(&next_height) {
            let height = data.next_height;
            data.next_height += 1;
            return Some((height, block));
        }
        None
    }
    pub async fn get_next_block(&mut self) -> Option<(u32, Bytes)> {
        loop {
            let data = self.try_get_next_block();
            if data.is_some() {
                return data;
            }
            if self.is_all_read() {
                return None;
            }
            sleep(Duration::from_millis(100)).await;
        }
    }
}

