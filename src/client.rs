
use std::io::{
    Read,
    Cursor,
};
use std::sync::{
    Arc,
};
use redis::Commands;
use bitcoin::{
    VarInt,
    block::Block,
    consensus::{
        Encodable,
        Decodable,
        WriteExt,
    }
};

use crate::{
    Binary,
};

pub trait KVS: Send + Sync {
    fn get(&self, key: &str) -> Option<Binary>;
    fn set(&self, key: &str, value: &[u8]);
}

#[derive(Debug, Clone)]
pub struct RedisClientPool {
    pool: r2d2::Pool<redis::Client>,
}

impl RedisClientPool {
    pub fn new(redis_url: &str) -> Self {
        let client = redis::Client::open(redis_url).unwrap();
        let pool_size = std::thread::available_parallelism().unwrap().get();
        println!("Redis connection pool size: {}", pool_size);
        let pool = r2d2::Pool::builder().max_size(pool_size as u32).build(client).unwrap();
        Self {
            pool,
        }
    }
}

impl KVS for RedisClientPool {
    fn get(&self, key: &str) -> Option<Binary> {
        let value: Option<Binary> = self.pool.get().unwrap().get(&key).unwrap();
        value
    }
    fn set(&self, key: &str, value: &[u8]) {
        let _: () = self.pool.get().unwrap().set(key, value).unwrap();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KVSTxData {
    lens: Vec<usize>,
    txdata: Binary,
}

impl KVSTxData {
    pub fn new<R: Read + bitcoin::io::Read>(read: &mut R) -> Self {
        // Get the number of transactions.
        let tx_len = VarInt::consensus_decode(read).unwrap().0 as usize;
        // Read transaction sizes.
        let mut lens = Vec::with_capacity(tx_len);
        for _ in 0..tx_len {
            let tx_size = VarInt::consensus_decode(read).unwrap().0 as usize;
            lens.push(tx_size);
        }
        let mut txdata = Vec::new();
        read.read_to_end(&mut txdata).unwrap();
        Self {
            lens,
            txdata,
        }
    }
    pub fn len(&self) -> usize {
        self.lens.len()
    }
    // Get the i-th transaction.
    pub fn get(&self, index: usize) -> Option<&[u8]> {
        if index >= self.lens.len() {
            return None;
        }
        let start = self.lens.iter().take(index).sum::<usize>();
        let end = start + self.lens[index];
        Some(&self.txdata[start..end])
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KVSBlock {
    header: [u8; 80],
    txdata: KVSTxData,
}

impl Encodable for KVSBlock {
    fn consensus_encode<W: bitcoin::io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, bitcoin::io::Error> {
        let mut written = 0;
        writer.emit_slice(&self.header)?;
        written += 80;
        let tx_len = VarInt::from(self.txdata.len());
        tx_len.consensus_encode(writer)?;
        written += tx_len.size();
        writer.emit_slice(&self.txdata.txdata)?;
        written += self.txdata.txdata.len();
        Ok(written)
    }
}

impl Decodable for KVSBlock {
    fn consensus_decode<R: bitcoin::io::Read + ?Sized>(reader: &mut R) -> Result<Self, bitcoin::consensus::encode::Error> {
        let block = Block::consensus_decode(reader)?;
        let mut header = [0u8; 80];
        block.header.consensus_encode(&mut header.as_mut()).unwrap();
        let (lens, txdata): (Vec<usize>, Vec<Binary>) = block.txdata.iter().map(|tx| {
            let mut tx_vec = Vec::new();
            tx.consensus_encode(&mut tx_vec).unwrap();
            (tx_vec.len(), tx_vec)
        }).unzip();
        let txdata = KVSTxData {
            lens,
            txdata: txdata.concat(),
        };
        Ok(Self {
            header,
            txdata,
        })
    }
}

impl TryFrom<Binary> for KVSBlock {
    type Error = std::io::Error;
    fn try_from(block: Binary) -> Result<Self, Self::Error> {
        let mut cursor = Cursor::new(block);
        let mut header = [0u8; 80];
        cursor.read_exact(&mut header)?;
        let txdata = KVSTxData::new(&mut cursor);
        Ok(Self {
            header,
            txdata,
        })
    }
}

impl From<KVSBlock> for Binary {
    fn from(block: KVSBlock) -> Self {
        let mut block_vec = block.header.to_vec();
        let tx_len = VarInt::from(block.txdata.len());
        tx_len.consensus_encode(&mut block_vec).unwrap();
        for len in block.txdata.lens {
            let tx_len = VarInt::from(len);
            tx_len.consensus_encode(&mut block_vec).unwrap();
        }
        block_vec.extend(block.txdata.txdata);
        block_vec
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
    fn get(&self, key_prefix: &str, key: &str) -> Option<Binary> {
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
        let height_vec: Option<Binary> = self.kvs.get(format!("{}:{}:nextBlockHeight", self.prefix, self.chain).as_str());
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
    pub fn get_transaction(&self, tx_hash: &[u8; 32]) -> Option<Binary> {
        self.get("transaction", hex::encode(tx_hash).as_str())
    }
    pub fn add_block(&self, height: u32, block_bytes: Binary, set_next_block_height: Option<bool>) {
        let block = Block::consensus_decode(&mut block_bytes.as_slice()).unwrap();
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
    
    #[derive(Debug, Clone)]
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
    
    pub fn load_blocks() -> Vec<Binary> {
        let mut blocks: Vec<Binary> = Vec::new();
        for height in 0..1000 {
            let mut f = File::open(format!("./fixture/blocks/block_{}.bin", height)).expect(format!("block_{}.bin file not found", height).as_str());
            let mut block = Vec::new();
            f.read_to_end(&mut block).expect("Something went wrong reading the file");
            blocks.push(block);
        }
        blocks
    }
    
    mod kvs_block {
        use super::*;
        #[test]
        fn consensus_decode() {
            let blocks = load_blocks();
            let kvs_block = KVSBlock::consensus_decode(&mut blocks[0].as_slice()).unwrap();
            let block = Block::consensus_decode(&mut blocks[0].as_slice()).unwrap();
            // Check header.
            let mut block_header_vec = [0u8; 80];
            block.header.consensus_encode(&mut block_header_vec.as_mut()).unwrap();
            assert_eq!(kvs_block.header, block_header_vec);
            // Check transactions.
            assert_eq!(kvs_block.txdata.len(), block.txdata.len());
            for i in 0..block.txdata.len() {
                let mut tx_vec = Vec::new();
                block.txdata[i].consensus_encode(&mut tx_vec).unwrap();
                assert_eq!(kvs_block.txdata.get(i), Some(tx_vec.as_slice()));
            }
        }
        #[test]
        fn consensus_encode() {
            let blocks = load_blocks();
            let block = Block::consensus_decode(&mut blocks[0].as_slice()).unwrap();
            let mut block_header_vec = [0u8; 80];
            block.header.consensus_encode(&mut block_header_vec.as_mut()).unwrap();
            let kvs_block = KVSBlock::consensus_decode(&mut blocks[0].as_slice()).unwrap();
            let mut kvs_block_vec = Vec::new();
            kvs_block.consensus_encode(&mut kvs_block_vec).unwrap();
            assert_eq!(hex::encode(&kvs_block_vec), hex::encode(&blocks[0]));
        }
        #[test]
        fn encode_decode() {
            let blocks = load_blocks();
            let kvs_block = KVSBlock::consensus_decode(&mut blocks[0].as_slice()).unwrap();
            let kvs_block_vec: Binary = kvs_block.clone().into();
            let kvs_block_decoded: KVSBlock = kvs_block_vec.try_into().unwrap();
            assert_eq!(kvs_block, kvs_block_decoded);
        }
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

