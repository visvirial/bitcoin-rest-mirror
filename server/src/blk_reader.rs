
use std::io::prelude::*;
use std::io::BufReader;
use std::fs::File;
use std::sync::{
    Arc,
    RwLock,
};
use std::collections::{
    HashMap,
    VecDeque,
};
use bytes::Bytes;
use tokio::time::{
    sleep,
    Duration,
};
use bitcoin::block::{
    Header,
};
use bitcoin::consensus::{
    Decodable,
    //Encodable,
};

#[derive(Clone)]
pub struct BlkReaderData {
    next_index: u32,
    unprocessed_blocks: Vec<Bytes>,
    // Block hash -> block.
    blocks_by_hash: HashMap<[u8; 32], Bytes>,
    // Parent hash -> block hash.
    parents: HashMap<[u8; 32], [u8; 32]>,
    // Block height -> block.
    blocks_by_height: VecDeque<Bytes>,
    next_height: u32,
    current_hash: [u8; 32],
    all_read: bool,
}

impl BlkReaderData {
    pub fn new() -> Self {
        Self {
            next_index: 0,
            unprocessed_blocks: Vec::new(),
            blocks_by_hash: HashMap::new(),
            parents: HashMap::new(),
            blocks_by_height: VecDeque::new(),
            next_height: 0,
            current_hash: [0u8; 32],
            all_read: false,
        }
    }
    pub fn registered_block_count(&self) -> usize {
        self.unprocessed_blocks.len() +
        self.blocks_by_hash.len() +
        self.blocks_by_height.len()
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
    pub fn is_all_read(&self) -> bool {
        self.data.read().unwrap().all_read
    }
    pub fn set_max_blocks(mut self, max_blocks: u32) -> Self {
        self.max_blocks = max_blocks;
        self
    }
    pub fn registered_block_count(&self) -> usize {
        self.data.read().unwrap().registered_block_count()
    }
    pub fn get_next_height(&self) -> u32 {
        self.data.read().unwrap().next_height
    }
    pub fn processed_blocks_count(&self) -> usize {
        self.data.read().unwrap().blocks_by_height.len()
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
            // Save blcok.
            self.data.write().unwrap().unprocessed_blocks.push(Bytes::from(block_vec));
        }
    }
    pub fn read_next_file(&mut self) -> Result<u32, ()> {
        let next_index = {
            let mut data = self.data.write().unwrap();
            let next_index = data.next_index;
            data.next_index += 1;
            next_index
        };
        let block_count = self.read_file(next_index);
        if block_count.is_err() {
            return Err(());
        }
        Ok(block_count.unwrap())
    }
    pub fn process_blocks(&mut self) {
        // Decode blocks.
        loop {
            let mut data = self.data.write().unwrap();
            let block_bytes = data.unprocessed_blocks.pop();
            if block_bytes.is_none() {
                break;
            }
            let block_bytes = block_bytes.unwrap();
            let block_header = Header::consensus_decode(&mut block_bytes.as_ref());
            if block_header.is_err() {
                continue;
            }
            let block_header = block_header.unwrap();
            let block_hash: [u8; 32] = *block_header.block_hash().as_ref();
            data.blocks_by_hash.insert(block_hash, block_bytes);
            data.parents.insert(*block_header.prev_blockhash.as_ref(), block_hash);
        }
        // Find child block.
        loop {
            let mut data = self.data.write().unwrap();
            let current_hash = data.current_hash;
            let child_hash = data.parents.remove(&current_hash);
            if child_hash.is_none() {
                return;
            }
            let child_hash = child_hash.unwrap();
            let child_block = data.blocks_by_hash.remove(&child_hash).unwrap();
            data.blocks_by_height.push_back(child_block);
            data.current_hash = child_hash;
        }
    }
    pub fn read_and_process_next_file(&mut self) -> Result<u32, ()> {
        let block_count = self.read_next_file();
        if block_count.is_err() {
            return Err(());
        }
        self.process_blocks();
        Ok(block_count.unwrap())
    }
    pub async fn run_threads(&mut self, concurrency: usize) {
        let mut handles = Vec::new();
        for _ in 0..concurrency {
            let mut this = self.clone();
            let handle = tokio::spawn(async move {
                loop {
                    if this.registered_block_count() >= this.max_blocks as usize {
                        //println!("Max blocks reached.");
                        sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                    if this.read_and_process_next_file().is_err() {
                        break;
                    }
                }
            });
            handles.push(handle);
        }
        {
            let mut this = self.clone();
            tokio::spawn(async move {
                futures::future::join_all(handles).await;
                this.process_blocks();
                this.data.write().unwrap().all_read = true;
            });
        }
    }
    pub fn get_next_block(&mut self) -> Option<(u32, Bytes)> {
        let mut data = self.data.write().unwrap();
        if let Some(block) = data.blocks_by_height.pop_front() {
            let height = data.next_height;
            data.next_height += 1;
            return Some((height, block));
        }
        None
    }
}

