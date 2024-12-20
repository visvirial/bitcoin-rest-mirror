
use std::sync::{
    Arc,
};
use redis::Commands;
use bitcoin::consensus::Encodable;

pub trait KVS: Send + Sync {
    fn get_key(&self, prefix: &str, chain: &str, key_prefix: &str, key: &str) -> String {
        format!("{}:{}:{}:{}", prefix, chain, key_prefix, key)
    }
    fn get(&self, prefix: &str, key: &str) -> Option<Vec<u8>>;
    fn set(&self, prefix: &str, key: &str, value: &[u8]);
}

#[derive(Clone)]
pub struct RedisClientPool {
    prefix: String,
    chain: String,
    pool: Arc<r2d2::Pool<redis::Client>>,
}

impl RedisClientPool {
    pub fn new(redis_url: &str, chain: String, prefix: Option<String>) -> Self {
        let client = redis::Client::open(redis_url).unwrap();
        let pool_size = std::thread::available_parallelism().unwrap().get();
        println!("Redis connection pool size: {}", pool_size);
        let pool = Arc::new(r2d2::Pool::builder().max_size(pool_size as u32).build(client).unwrap());
        Self {
            prefix: prefix.unwrap_or("bitcoin-rest-mirror".to_string()),
            chain,
            pool,
        }
    }
}

impl KVS for RedisClientPool {
    fn get(&self, key_prefix: &str, key: &str) -> Option<Vec<u8>> {
        let key = self.get_key(&self.prefix, &self.chain, key_prefix, key);
        let value: Option<Vec<u8>> = self.pool.get().unwrap().get(&key).unwrap();
        value
    }
    fn set(&self, key_prefix: &str, key: &str, value: &[u8]) {
        let key = self.get_key(&self.prefix, &self.chain, key_prefix, key);
        let _: () = self.pool.get().unwrap().set(key, value).unwrap();
    }
}

#[derive(Clone)]
pub struct Client {
    kvs: Arc<dyn KVS>,
}

impl Client {
    pub fn new(kvs: impl KVS + 'static) -> Self {
        Self {
            kvs: Arc::new(kvs),
        }
    }
    pub fn get(&mut self, key_prefix: &str, key: &str) -> Option<Vec<u8>> {
        self.kvs.get(key_prefix, key)
    }
    pub fn set(&mut self, prefix: &str, key: &str, value: &[u8]) {
        self.kvs.set(prefix, key, value);
    }
    pub fn get_transaction(&mut self, tx_hash: &[u8; 32]) -> Option<Vec<u8>> {
        self.get("transaction", hex::encode(tx_hash).as_str())
    }
    pub fn set_transaction(&mut self, tx_hash: &[u8; 32], tx: &[u8]) {
        self.set("transaction", hex::encode(tx_hash).as_str(), tx);
    }
    pub fn get_block_header(&mut self, block_hash: &[u8; 32]) -> Option<[u8; 80]> {
        let block_header = self.get("blockHeader", hex::encode(block_hash).as_str());
        match block_header {
            Some(block_header) => {
                let mut block_header_array = [0u8; 80];
                block_header_array.copy_from_slice(&block_header);
                Some(block_header_array)
            },
            None => None
        }
    }
    pub fn get_block_transaction_hashes(&mut self, block_hash: &[u8; 32]) -> Option<Vec<[u8; 32]>> {
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
    pub fn get_block(&mut self, block_hash: &[u8; 32]) -> Option<Vec<u8>> {
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

/*
#[cfg(test)]
mod tests {
    use super::*;
    use redis_test::MockRedisConnection;
    
    struct RedisMockConnectionManager {
        connection: MockRedisConnection,
    }
    
    impl r2d2::ManageConnection for RedisMockConnectionManager {
        type Connection = MockRedisConnection;
        type Error = redis::RedisError;
        
        fn connect(&self) -> Result<Self::Connection, Self::Error> {
            Ok(self.connection.clone())
        }
        
        fn is_valid(&self, _conn: &mut Self::Connection) -> Result<(), Self::Error> {
            Ok(())
        }
        
        fn has_broken(&self, _conn: &mut Self::Connection) -> bool {
            false
        }
    }
    
    fn create_client() -> Client {
        let pool_size = 1;
        let redis_mock_connection_manager = RedisMockConnectionManager {
            connection: MockRedisConnection::new(vec![]),
        };
        let pool = Arc::new(r2d2::Pool::builder().max_size(pool_size as u32).build(redis_mock_connection_manager).unwrap());
        let redis_client_pool = RedisClientPool {
            prefix: "bitcoin-rest-mirror".to_string(),
            chain: "BTC".to_string(),
            pool,
        };
        let client = Client::new(redis_client_pool);
        client
    }
    
    #[test]
    fn test_get_set() {
    }
}
*/

