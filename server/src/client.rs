
use std::sync::{
    Arc,
};
use redis::Commands;
use bitcoin::consensus::Encodable;

pub trait KVS: Send + Sync {
    fn get(&self, key: &str) -> Option<Vec<u8>>;
    fn set(&self, key: &str, value: &[u8]);
}

#[derive(Clone)]
pub struct RedisClientPool {
    pool: Arc<r2d2::Pool<redis::Client>>,
}

impl RedisClientPool {
    pub fn new(redis_url: &str) -> Self {
        let client = redis::Client::open(redis_url).unwrap();
        let pool_size = std::thread::available_parallelism().unwrap().get();
        println!("Redis connection pool size: {}", pool_size);
        let pool = Arc::new(r2d2::Pool::builder().max_size(pool_size as u32).build(client).unwrap());
        Self {
            pool,
        }
    }
}

impl KVS for RedisClientPool {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        let value: Option<Vec<u8>> = self.pool.get().unwrap().get(&key).unwrap();
        value
    }
    fn set(&self, key: &str, value: &[u8]) {
        let _: () = self.pool.get().unwrap().set(key, value).unwrap();
    }
}

#[derive(Clone)]
pub struct Client {
    prefix: String,
    chain: String,
    kvs: Arc<dyn KVS>,
}

impl Client {
    pub fn new(kvs: impl KVS + 'static, chain: String, prefix: Option<String>) -> Self {
        Self {
            prefix: prefix.unwrap_or("bitcoin-rest-mirror".to_string()),
            chain,
            kvs: Arc::new(kvs),
        }
    }
    fn get_key(&self, key_prefix: &str, key: &str) -> String {
        format!("{}:{}:{}:{}", self.prefix, self.chain, key_prefix, key)
    }
    fn get(&self, key_prefix: &str, key: &str) -> Option<Vec<u8>> {
        let key = self.get_key(key_prefix, key);
        self.kvs.get(&key)
    }
    fn set(&self, key_prefix: &str, key: &str, value: &[u8]) {
        let key = self.get_key(key_prefix, key);
        self.kvs.set(&key, &value);
    }
    fn height_to_slice(height: u32) -> [u8; 4] {
        let mut height_vec = [0u8; 4];
        height_vec.copy_from_slice(&height.to_le_bytes());
        height_vec
    }
    fn slice_to_height(height_vec: &[u8; 4]) -> u32 {
        u32::from_le_bytes(*height_vec)
    }
    pub fn set_next_block_height(&self, height: u32) {
        self.kvs.set(format!("{}:{}:nextBlockHeight", self.prefix, self.chain).as_str(), &Client::height_to_slice(height));
    }
    pub fn get_next_block_height(&self) -> u32 {
        let height_vec: Option<Vec<u8>> = self.kvs.get(format!("{}:{}:nextBlockHeight", self.prefix, self.chain).as_str());
        match height_vec {
            Some(height_vec) => {
                Client::slice_to_height(&height_vec.try_into().unwrap())
            },
            None => 0,
        }
    }
    pub fn set_block_header(&self, block_hash: &[u8; 32], block_header: &[u8; 80]) {
        self.set("blockHeader", hex::encode(block_hash).as_str(), block_header);
    }
    pub fn get_block_header(&self, block_hash: &[u8; 32]) -> Option<[u8; 80]> {
        let block_header = self.get("blockHeader", hex::encode(block_hash).as_str());
        match block_header {
            Some(block_header) => {
                Some(block_header.try_into().unwrap())
            },
            None => None
        }
    }
    pub fn set_block_hash_by_height(&self, height: u32, block_hash: &[u8; 32]) {
        self.set("blockHashByHeight", height.to_string().as_str(), block_hash);
    }
    pub fn get_block_hash_by_height(&self, height: u32) -> Option<[u8; 32]> {
        let block_hash = self.get("blockHashByHeight", height.to_string().as_str());
        match block_hash {
            Some(block_hash) => {
                Some(block_hash.try_into().unwrap())
            },
            None => None
        }
    }
    pub fn set_transaction(&self, tx_hash: &[u8; 32], tx: &[u8]) {
        self.set("transaction", hex::encode(tx_hash).as_str(), tx);
    }
    pub fn get_transaction(&self, tx_hash: &[u8; 32]) -> Option<Vec<u8>> {
        self.get("transaction", hex::encode(tx_hash).as_str())
    }
    pub fn get_block_transaction_hashes(&self, block_hash: &[u8; 32]) -> Option<Vec<[u8; 32]>> {
        let tx_hashes = self.get("blockTransactionHashes", hex::encode(block_hash).as_str());
        match tx_hashes {
            Some(tx_hashes) => {
                let mut tx_hashes_array = Vec::new();
                for i in 0..(tx_hashes.len() / 32) {
                    let mut tx_hash = [0u8; 32];
                    tx_hash.copy_from_slice(&tx_hashes[(i * 32)..((i + 1) * 32)]);
                    tx_hashes_array.push(tx_hash);
                }
                Some(tx_hashes_array)
            },
            None => None
        }
    }
    pub fn get_block(&self, block_hash: &[u8; 32]) -> Option<Vec<u8>> {
        let block_header = match self.get_block_header(block_hash) {
            Some(block_header) => block_header,
            None => return None
        };
        let tx_hashes = match self.get_block_transaction_hashes(block_hash) {
            Some(tx_hashes) => tx_hashes,
            None => return None
        };
        let txs = tx_hashes.iter().map(|tx_hash| self.get_transaction(tx_hash)).collect::<Vec<Option<Vec<u8>>>>();
        if txs.iter().any(|tx| tx.is_none()) {
            return None;
        }
        let txs = txs.into_iter().flatten().collect::<Vec<Vec<u8>>>();
        let tx_length_varint = bitcoin::VarInt::from(txs.len());
        let mut tx_length_vec = Vec::new();
        tx_length_varint.consensus_encode(&mut tx_length_vec).unwrap();
        let mut block = Vec::new();
        block.extend(block_header);
        block.extend(tx_length_vec);
        block.extend(txs.iter().flatten());
        Some(block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    use std::fs::File;
    use std::io::Read;
    use std::sync::Mutex;
    use std::collections::HashMap;
    use bitcoin::block::Block;
    use bitcoin::consensus::Decodable;
    
    #[derive(Clone)]
    struct MockKVS {
        db: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    }
    
    impl MockKVS {
        pub fn new() -> Self {
            Self {
                db: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }
    
    impl KVS for MockKVS {
        fn get(&self, key: &str) -> Option<Vec<u8>> {
            let value: Option<Vec<u8>> = self.db.lock().unwrap().get(key).cloned();
            value
        }
        fn set(&self, key: &str, value: &[u8]) {
            self.db.lock().unwrap().insert(key.to_string(), value.to_vec());
        }
    }
    
    fn create_client() -> Client {
        let mock_kvs = MockKVS::new();
        let client = Client::new(mock_kvs, "BTC".to_string(), None);
        client
    }
    
    fn load_blocks() -> Vec<Vec<u8>> {
        let mut blocks: Vec<Vec<u8>> = Vec::new();
        for height in 0..1000 {
            let mut f = File::open(format!("../test/fixtures/block_{}.bin", height)).expect(format!("block_{}.bin file not found", height).as_str());
            let mut block = Vec::new();
            f.read_to_end(&mut block).expect("Something went wrong reading the file");
            blocks.push(block);
        }
        blocks
    }
    
    mod next_block_height {
        use super::*;
        #[test]
        fn get_first() {
            let client = create_client();
            assert_eq!(client.get_next_block_height(), 0);
        }
        #[test]
        fn set() {
            let client = create_client();
            let height: u32 = 1234;
            client.set_next_block_height(height);
            assert_eq!(client.get_next_block_height(), height);
        }
    }
    
    mod block_header {
        use super::*;
        #[test]
        fn get_none() {
            let client = create_client();
            let block_hash = [0u8; 32];
            assert_eq!(client.get_block_header(&block_hash), None);
        }
        #[test]
        fn set() {
            let client = create_client();
            let blocks = load_blocks();
            let block = Block::consensus_decode(&mut blocks[0].as_slice()).unwrap();
            let block_hash = block.block_hash();
            let block_hash_slice: [u8; 32] = *block_hash.as_ref();
            let block_header = [0u8; 80];
            assert_eq!(block.header.consensus_encode(&mut block_header.to_vec()).unwrap(), 80);
            client.set_block_header(&block_hash_slice, &block_header);
            assert_eq!(client.get_block_header(&block_hash_slice), Some(block_header));
        }
    }
    
    mod block_hash_by_height {
        use super::*;
        #[test]
        fn get_none() {
            let client = create_client();
            assert_eq!(client.get_block_hash_by_height(0), None);
        }
        #[test]
        fn set() {
            let client = create_client();
            let blocks = load_blocks();
            let block = Block::consensus_decode(&mut blocks[0].as_slice()).unwrap();
            let block_hash = block.block_hash();
            let block_hash_slice: [u8; 32] = *block_hash.as_ref();
            client.set_block_hash_by_height(0, &block_hash_slice);
            assert_eq!(client.get_block_hash_by_height(0), Some(block_hash_slice));
        }
    }
    
}

