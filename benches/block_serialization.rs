
#[macro_use]
extern crate criterion;

use std::fs::File;
use std::io::Read;
use criterion::Criterion;
use bitcoin::block::Block;
use bitcoin::consensus::{
    Encodable,
    Decodable,
};

use bitcoin_rest_mirror::client::KVSBlock;

pub fn load_block_800_000() -> bytes::Bytes {
    let mut f = File::open("./fixture/blocks/block_800000.bin").expect("block_800000.bin file not found");
    let mut block = Vec::new();
    f.read_to_end(&mut block).expect("Something went wrong reading the file");
    block.into()
}

fn criterion_benchmark(c: &mut Criterion) {
    let block_bytes = load_block_800_000();
    let block_bitcoin = Block::consensus_decode(&mut block_bytes.as_ref()).unwrap();
    let block_kvs = KVSBlock::consensus_decode(&mut block_bytes.as_ref()).unwrap();
    let block_kvs_bytes = bytes::Bytes::from(block_kvs.clone());
    c.bench_function("load block 800_000", |b| b.iter(|| {
        load_block_800_000();
    }));
    c.bench_function("rust-bitcoin: consensus_encode", |b| b.iter(|| {
        let mut vec = Vec::new();
        block_bitcoin.consensus_encode(&mut vec).unwrap();
    }));
    c.bench_function("rust-bitcoin: consensus_decode", |b| b.iter(|| {
        Block::consensus_decode(&mut block_bytes.as_ref()).unwrap();
    }));
    c.bench_function("KVSBlock: consensus_encode", |b| b.iter(|| {
        let mut vec = Vec::new();
        block_kvs.consensus_encode(&mut vec).unwrap();
    }));
    c.bench_function("KVSBlock: consensus_decode", |b| b.iter(|| {
        KVSBlock::consensus_decode(&mut block_bytes.as_ref()).unwrap();
    }));
    c.bench_function("KVSBlock: try_from Bytes", |b| b.iter(|| {
        KVSBlock::try_from(block_kvs_bytes.clone()).unwrap();
    }));
    c.bench_function("KVSBlock: into Bytes", |b| b.iter(|| {
        let _bytes: bytes::Bytes = block_kvs.clone().into();
    }));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

