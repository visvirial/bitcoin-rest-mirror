
use std::time::{
    SystemTime,
    Duration,
};
use std::thread::{
    available_parallelism,
    sleep,
};
use std::sync::{
    Arc,
    RwLock,
};
use std::collections::HashMap;
use bytes::Bytes;
use num_format::{
    Locale,
    ToFormattedString,
};
use rayon::prelude::*;
use reqwest::{
    Response,
    StatusCode,
};
use bitcoin_hashes::Sha256d;

#[derive(Clone)]
pub struct BitcoinRest {
    client: reqwest::Client,
    rest_endpoint: String,
}

impl BitcoinRest {
    pub fn new(rest_endpoint: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            rest_endpoint: rest_endpoint.unwrap_or("http://localhost:8332/rest".to_string()),
        }
    }
    pub async fn fetch(&self, path: &[&str], ext: &str, query: Option<&str>) -> Response {
        let mut url = format!("{}/{}.{}", self.rest_endpoint, path.join("/"), ext);
        if let Some(query) = query {
            url.push_str(&format!("?{}", query));
        }
        let response = self.client.get(&url).send().await.unwrap();
        response
    }
    pub async fn fetch_hex(&self, path: &[&str], query: Option<&str>) -> Result<String, Response> {
        let response = self.fetch(path, "hex", query).await;
        if response.status() != StatusCode::OK {
            return Err(response);
        }
        let hex = response.text().await.unwrap().trim().to_string();
        Ok(hex)
    }
    pub async fn fetch_bin(&self, path: &[&str], query: Option<&str>) -> Result<Bytes, Response> {
        let response = self.fetch(path, "bin", query).await;
        if response.status() != StatusCode::OK {
            return Err(response);
        }
        let bytes = response.bytes().await.unwrap();
        Ok(bytes)
    }
    pub async fn get_block(&self, mut hash: [u8; 32]) -> Result<Bytes, Response> {
        hash.reverse();
        let block = self.fetch_bin(&["block", &hex::encode(hash)], None).await?;
        Ok(block)
    }
    pub async fn get_blockhashbyheight(&self, height: u32) -> Result<[u8; 32], Response> {
        let block_hash = self.fetch_bin(&["blockhashbyheight", &height.to_string()], None).await?;
        if block_hash.len() != 32 {
            panic!("Invalid block hash length");
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&block_hash);
        Ok(hash)
    }
    pub async fn get_headers(&self, mut hash: [u8; 32], count: u32) -> Result<Vec<[u8; 80]>, Response> {
        hash.reverse();
        let mut headers = Vec::new();
        let headers_bytes = self.fetch_bin(&["headers", &hex::encode(hash)], Some(format!("count={}", count).as_str())).await?;
        if headers_bytes.len() % 80 != 0 {
            panic!("Invalid headers length");
        }
        for i in 0..(headers_bytes.len() / 80) {
            let mut header = [0u8; 80];
            header.copy_from_slice(&headers_bytes[(i * 80)..((i + 1) * 80)]);
            headers.push(header);
        }
        Ok(headers)
    }
    pub async fn get_all_headers(&self, mut hash: [u8; 32], count: Option<u32>) -> Result<Vec<[u8; 80]>, Response> {
        let mut result = Vec::new();
        let count = count.unwrap_or(2000);
        let mut is_first = true;
        loop {
            let mut headers = self.get_headers(hash, count).await?;
            let headers_len = headers.len();
            if headers_len == 0 {
                break;
            }
            // Drop first header on non-first iteration.
            if !is_first {
                headers = headers[1..].to_vec();
            }
            is_first = false;
            hash = Sha256d::hash(headers.last().unwrap()).to_byte_array();
            result.push(headers);
            if headers_len < count as usize {
                break;
            }
        }
        let headers = result.concat();
        Ok(headers)
    }
}

struct BlockDownloaderData {
    current_height: u32,
    max_height: u32,
    blocks: HashMap<u32, Bytes>,
}

impl BlockDownloaderData {
    pub fn new() -> Self {
        Self {
            current_height: 0,
            max_height: 0,
            blocks: HashMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct BlockDownloader {
    bitcoin_rest: BitcoinRest,
    concurrency: usize,
    max_blocks: u32,
    data: Arc<RwLock<BlockDownloaderData>>,
}

impl BlockDownloader {
    pub fn new(rest_endpoint: Option<String>) -> Self {
        let bitcoin_rest = BitcoinRest::new(rest_endpoint);
        let concurrency = available_parallelism().unwrap().get();
        let data = BlockDownloaderData::new();
        Self {
            bitcoin_rest,
            concurrency,
            max_blocks: 1000,
            data: Arc::new(RwLock::new(data)),
        }
    }
    pub fn set_concurrency(&mut self, concurrency: usize) {
        self.concurrency = concurrency;
    }
    pub fn set_max_blocks(&mut self, max_blocks: u32) {
        self.max_blocks = max_blocks;
    }
    pub fn try_shift(&mut self) -> Option<(u32, Bytes)> {
        let data = &mut self.data.write().unwrap();
        let current_height = data.current_height;
        if data.blocks.contains_key(&current_height) {
            let block = data.blocks.remove(&current_height).unwrap();
            data.current_height += 1;
            return Some((current_height, block));
        }
        None
    }
    pub fn shift(&mut self) -> Option<(u32, Bytes)> {
        {
            let data = &self.data.read().unwrap();
            if data.current_height > data.max_height {
                return None;
            }
        }
        loop {
            if let Some(block) = self.try_shift() {
                return Some(block);
            }
            sleep(Duration::from_millis(100));
        }
    }
    pub async fn run(&mut self, start_height: u32) -> Result<(), Response> {
        let first_block_hash = self.bitcoin_rest.get_blockhashbyheight(start_height).await;
        if first_block_hash.is_err() {
            return Ok(());
        }
        let first_block_hash = first_block_hash.unwrap();
        // Fetch all headers.
        println!("Fetching all block headers...");
        let start_time = SystemTime::now();
        let headers = self.bitcoin_rest.get_all_headers(first_block_hash, None).await?;
        let blocks_len = headers.len();
        println!("Fetched {} block headers in {}ms.", blocks_len, start_time.elapsed().unwrap().as_millis());
        let start_time = SystemTime::now();
        let block_hashes = headers.par_iter().map(|header| Sha256d::hash(header).to_byte_array()).collect::<Vec<[u8; 32]>>();
        println!("Computed block hashes in {}ms.", start_time.elapsed().unwrap().as_millis());
        self.data.write().unwrap().current_height = start_height;
        self.data.write().unwrap().max_height = start_height + blocks_len as u32 - 1;
        println!("Fetching blocks...");
        for height in start_height..(start_height + blocks_len as u32) {
            loop {
                if self.data.read().unwrap().blocks.len() >= self.max_blocks as usize {
                    sleep(Duration::from_millis(100));
                } else {
                    break;
                }
            }
            let block_hash = block_hashes[(height - start_height) as usize];
            let block = self.bitcoin_rest.get_block(block_hash).await?;
            self.data.write().unwrap().blocks.insert(height, block);
        }
        Ok(())
    }
    pub fn run_spawn(&mut self, start_height: u32) {
        let mut downloader = self.clone();
        tokio::spawn(async move {
            downloader.run(start_height).await.unwrap();
        });
    }
}

#[tokio::main]
async fn main() {
    let mut downloader = BlockDownloader::new(None);
    downloader.run_spawn(0);
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

