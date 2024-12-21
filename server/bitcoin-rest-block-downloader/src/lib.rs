
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
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(1))
            .build()
            .unwrap()
            ;
        Self {
            client,
            rest_endpoint: rest_endpoint.unwrap_or("http://localhost:8332/rest".to_string()),
        }
    }
    pub async fn fetch(&self, path: &[&str], ext: &str, query: Option<&str>) -> Response {
        let mut url = format!("{}/{}.{}", self.rest_endpoint, path.join("/"), ext);
        if let Some(query) = query {
            url.push_str(&format!("?{}", query));
        }
        let max_retries = 10;
        for _ in 0..max_retries {
            match self.client.get(&url).send().await {
                Ok(response) => {
                    return response;
                },
                Err(_) => {
                    println!("Fetch timeouted for: {}.", url);
                    sleep(Duration::from_millis(1000));
                },
            };
        }
        panic!("Failed to fetch {} after {} retries.", url, max_retries);
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
    next_height: u32,
    max_height: u32,
    blocks: HashMap<u32, Bytes>,
}

impl BlockDownloaderData {
    pub fn new() -> Self {
        Self {
            current_height: 0,
            next_height: 0,
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
    pub fn set_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency;
        self
    }
    pub fn set_max_blocks(mut self, max_blocks: u32) -> Self {
        self.max_blocks = max_blocks;
        self
    }
    pub fn get_current_height(&self) -> u32 {
        self.data.read().unwrap().current_height
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
        let block_hashes = Arc::new(RwLock::new(block_hashes));
        println!("Computed block hashes in {}ms.", start_time.elapsed().unwrap().as_millis());
        self.data.write().unwrap().current_height = start_height;
        self.data.write().unwrap().max_height = start_height + blocks_len as u32 - 1;
        println!("Fetching blocks with {} threads...", self.concurrency);
        self.data.write().unwrap().next_height = start_height;
        for _ in 0..self.concurrency {
            let downloader = self.clone();
            let block_hashes = block_hashes.clone();
            tokio::spawn(async move {
                loop {
                    // Sleep until blocks are consumed.
                    loop {
                        let max_blocks_reached = {
                            downloader.data.read().unwrap().blocks.len() >= downloader.max_blocks as usize
                        };
                        if max_blocks_reached {
                            sleep(Duration::from_millis(100));
                        } else {
                            break;
                        }
                    }
                    let height = {
                        let mut data = downloader.data.write().unwrap();
                        let next_height = data.next_height;
                        if next_height > data.max_height {
                            break;
                        }
                        data.next_height += 1;
                        next_height
                    };
                    let block_hash = block_hashes.read().unwrap()[(height - start_height) as usize];
                    let block = downloader.bitcoin_rest.get_block(block_hash).await;
                    if block.is_err() {
                        println!("Failed to fetch block {}.", height);
                    }
                    let block = block.unwrap();
                    downloader.data.write().unwrap().blocks.insert(height, block);
                }
            });
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

