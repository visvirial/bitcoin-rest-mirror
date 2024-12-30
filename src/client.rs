
use std::io::{
    Read,
    BufReader,
};
use std::sync::{
    Arc,
};
use bytes::{
    Bytes,
};
use redis::Commands;
use bitcoin::{
    VarInt,
    block::Block,
    consensus::{
        Encodable,
        Decodable,
    }
};

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
pub struct KVSBlock {
    header: [u8; 80],
    txdata: Vec<Bytes>,
}

impl Encodable for KVSBlock {
    fn consensus_encode<W: bitcoin::io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, bitcoin::io::Error> {
        let mut written = 0;
        writer.write_all(&self.header)?;
        written += 80;
        let tx_len = VarInt::from(self.txdata.len());
        let mut tx_len_vec = Vec::new();
        tx_len.consensus_encode(&mut tx_len_vec.as_mut())?;
        writer.write_all(&tx_len_vec)?;
        written += tx_len.size();
        for tx in &self.txdata {
            writer.write_all(tx.as_ref())?;
            written += tx.len();
        }
        Ok(written)
    }
}

impl Decodable for KVSBlock {
    fn consensus_decode<R: bitcoin::io::Read + ?Sized>(reader: &mut R) -> Result<Self, bitcoin::consensus::encode::Error> {
        let block = Block::consensus_decode(reader)?;
        let mut header = [0u8; 80];
        block.header.consensus_encode(&mut header.as_mut()).unwrap();
        let txdata = block.txdata.iter().map(|tx| {
            let mut tx_vec = Vec::new();
            tx.consensus_encode(&mut tx_vec).unwrap();
            Bytes::from(tx_vec)
        }).collect();
        Ok(Self {
            header,
            txdata,
        })
    }
}

impl TryFrom<Bytes> for KVSBlock {
    type Error = std::io::Error;
    fn try_from(block: Bytes) -> Result<Self, Self::Error> {
        let mut reader = BufReader::new(block.as_ref());
        let mut header = [0u8; 80];
        reader.read_exact(&mut header)?;
        let mut txdata: Vec<Bytes> = Vec::new();
        loop {
            let tx_size = VarInt::consensus_decode(&mut reader);
            if tx_size.is_err() {
                break;
            }
            let tx_size = tx_size.unwrap();
            let mut tx = vec![0u8; tx_size.0 as usize];
            reader.read_exact(&mut tx)?;
            txdata.push(Bytes::from(tx));
        }
        Ok(Self {
            header,
            txdata,
        })
    }
}

impl From<KVSBlock> for Bytes {
    fn from(block: KVSBlock) -> Self {
        let mut block_vec = block.header.to_vec();
        for tx in block.txdata {
            let tx_size = VarInt::from(tx.len());
            tx_size.consensus_encode(&mut block_vec).unwrap();
            block_vec.extend(tx.as_ref());
        }
        Bytes::from(block_vec)
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
        self.kvs.set(format!("{}:{}:nextBlockHeight", self.prefix, self.chain).as_str(), &Self::height_to_slice(height));
    }
    pub fn get_next_block_height(&self) -> u32 {
        let height_vec: Option<Vec<u8>> = self.kvs.get(format!("{}:{}:nextBlockHeight", self.prefix, self.chain).as_str());
        match height_vec {
            Some(height_vec) => {
                Self::slice_to_height(&height_vec.try_into().unwrap())
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
    pub fn set_block_height_by_hash(&self, block_hash: &[u8; 32], height: u32) {
        self.set("blockHeightByHash", hex::encode(block_hash).as_str(), &Self::height_to_slice(height));
    }
    pub fn get_block_height_by_hash(&self, block_hash: &[u8; 32]) -> Option<u32> {
        let height_vec = self.get("blockHeightByHash", hex::encode(block_hash).as_str());
        match height_vec {
            Some(height_vec) => {
                Some(Self::slice_to_height(&height_vec.try_into().unwrap()))
            },
            None => None
        }
    }
    pub fn set_block_transaction_hashes(&self, block_hash: &[u8; 32], tx_hashes: &Vec<[u8; 32]>) {
        let mut tx_hashes_vec: Vec<u8> = Vec::new();
        tx_hashes.iter().for_each(|e| tx_hashes_vec.extend(e));
        self.set("blockTransactionHashes", hex::encode(block_hash).as_str(), &tx_hashes_vec);
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
    pub fn set_transaction(&self, tx_hash: &[u8; 32], tx: &[u8]) {
        self.set("transaction", hex::encode(tx_hash).as_str(), tx);
    }
    pub fn get_transaction(&self, tx_hash: &[u8; 32]) -> Option<Vec<u8>> {
        self.get("transaction", hex::encode(tx_hash).as_str())
    }
    pub fn add_block(&self, height: u32, block_vec: Vec<u8>, set_next_block_height: Option<bool>) {
        let block = Block::consensus_decode(&mut block_vec.as_slice()).unwrap();
        let block_hash: [u8; 32] = *block.block_hash().as_ref();
        // Register transactions and hashes.
        let mut tx_hashes = Vec::new();
        for tx in block.txdata {
            let tx_hash: [u8; 32] = *tx.compute_txid().as_ref();
            tx_hashes.push(tx_hash);
            let mut tx_vec = Vec::new();
            tx.consensus_encode(&mut tx_vec).unwrap();
            self.set_transaction(&tx_hash, &tx_vec);
        }
        // Register block transaction hashes.
        self.set_block_transaction_hashes(&block_hash, &tx_hashes);
        // Register block header.
        let mut block_header = [0u8; 80];
        block.header.consensus_encode(&mut block_header.as_mut()).unwrap();
        self.set_block_header(&block_hash, &block_header);
        // Set block height by hash.
        self.set_block_height_by_hash(&block_hash, height);
        // Set block hash by height.
        self.set_block_hash_by_height(height, &block_hash);
        // Set next block height.
        let set_next_block_height = set_next_block_height.unwrap_or(true);
        if set_next_block_height {
            self.set_next_block_height(height + 1);
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
        let tx_length_varint = VarInt::from(txs.len());
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
pub mod tests {
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
    
    pub fn create_client() -> Client {
        let mock_kvs = MockKVS::new();
        let client = Client::new(mock_kvs, "BTC".to_string(), None);
        client
    }
    
    pub fn load_blocks() -> Vec<Vec<u8>> {
        let mut blocks: Vec<Vec<u8>> = Vec::new();
        for height in 0..1000 {
            let mut f = File::open(format!("./fixture/blocks/block_{}.bin", height)).expect(format!("block_{}.bin file not found", height).as_str());
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
            let block_hash: [u8; 32] = *block.block_hash().as_ref();
            let mut block_header = [0u8; 80];
            assert_eq!(block.header.consensus_encode(&mut block_header.as_mut()).unwrap(), 80);
            client.set_block_header(&block_hash, &block_header);
            assert_eq!(client.get_block_header(&block_hash), Some(block_header));
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
            let block_hash: [u8; 32] = *block.block_hash().as_ref();
            client.set_block_hash_by_height(0, &block_hash);
            assert_eq!(client.get_block_hash_by_height(0), Some(block_hash));
        }
    }
    
}

