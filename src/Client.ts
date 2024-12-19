
import Redis from 'ioredis';
import {
	Block,
} from 'bitcoinjs-lib';
import varuint from 'varuint-bitcoin';

export type RedisPrefix = 'blockHeader' | 'blockHashByHeight' | 'blockHeightByHash' | 'blockTransactionHashes' | 'transaction';

export class Client {
	
	private _client: Redis;
	
	constructor(
		public readonly redisUrl: string,
		public readonly chain = 'BTC',
		public readonly prefix = 'bitcoin-rest-mirror',
	) {
		this._client = new Redis(redisUrl);
		this._client.on('error', (err) => {
			throw new Error(`Redis error: ${err}`);
		});
	}
	
	public async destroy() {
		await this._client.disconnect();
	}
	
	public async set(prefix: RedisPrefix, key: string, value: Buffer) {
		await this._client.set(`${this.prefix}:${this.chain}:${prefix}:${key}`, value);
	}
	
	public async get(prefix: RedisPrefix, key: string): Promise<Buffer | null> {
		return await this._client.getBuffer(`${this.prefix}:${this.chain}:${prefix}:${key}`);
	}
	
	/**
	 * Low-level operations.
	 */
	
	public async setNextBlockHeight(height: number) {
		await this._client.set(`${this.prefix}:${this.chain}:nextBlockHeight`, height.toString());
	}
	
	public async getNextBlockHeight(): Promise<number> {
		const result = await this._client.get(`${this.prefix}:${this.chain}:nextBlockHeight`);
		if(!result) {
			return 0;
		}
		return +result;
	}
	
	public async setBlockHeader(hash: Buffer, header: Buffer) {
		if(header.length !== 80) {
			throw new Error('Invalid block header length');
		}
		await this.set('blockHeader', hash.toString('hex'), header);
	}
	
	public async getBlockHeader(hash: Buffer): Promise<Buffer | null> {
		return await this.get('blockHeader', hash.toString('hex'));
	}
	
	public async setBlockHashByHeight(height: number, hash: Buffer) {
		await this.set('blockHashByHeight', height.toString(), hash);
	}
	
	public async getBlockHashByHeight(height: number): Promise<Buffer | null> {
		return await this.get('blockHashByHeight', height.toString());
	}
	
	public async setBlockHeightByHash(height: number, hash: Buffer) {
		const heightBuffer = Buffer.alloc(4);
		heightBuffer.writeUInt32LE(height);
		await this.set('blockHeightByHash', hash.toString('hex'), heightBuffer);
	}
	
	public async getBlockHeightByHash(hash: Buffer): Promise<number | null> {
		const heightBuffer = await this.get('blockHeightByHash', hash.toString('hex'));
		if(heightBuffer === null) {
			return null;
		}
		return heightBuffer.readUInt32LE();
	}
	
	public async setBlockTransactionHashes(hash: Buffer, txHashes: Buffer[]) {
		await this.set('blockTransactionHashes', hash.toString('hex'), Buffer.concat(txHashes));
	}
	
	public async getBlockTransactionHashes(hash: Buffer): Promise<Buffer[] | null> {
		const result = await this.get('blockTransactionHashes', hash.toString('hex'));
		if(result === null) {
			return null;
		}
		const txHashes: Buffer[] = [];
		for(let i=0; i<result.length; i+=32) {
			txHashes.push(result.slice(i, i + 32));
		}
		return txHashes;
	}
	
	public async setTransaction(hash: Buffer, tx: Buffer) {
		await this.set('transaction', hash.toString('hex'), tx);
	}
	
	public async getTransaction(hash: Buffer): Promise<Buffer | null> {
		return await this.get('transaction', hash.toString('hex'));
	}
	
	/**
	 * High-level operations.
	 */
	
	public async addBlock(height: number, blockBuffer: Buffer, setNextBlockHeight = true) {
		const block = Block.fromBuffer(blockBuffer);
		const blockHash = block.getHash();
		if(block.transactions === undefined) {
			throw new Error('Block has no transactions');
		}
		// Register transactions and hashes.
		for(const tx of block.transactions) {
			await this.setTransaction(tx.getHash(), tx.toBuffer());
		}
		// Register block transaction hashes.
		await this.setBlockTransactionHashes(blockHash, block.transactions.map(tx => tx.getHash()));
		// Register block header.
		await this.setBlockHeader(blockHash, block.toBuffer(true));
		// Set block height by hash.
		await this.setBlockHeightByHash(height, blockHash);
		// Set block hash by height.
		await this.setBlockHashByHeight(height, blockHash);
		// Set next block height.
		if(setNextBlockHeight) {
			await this.setNextBlockHeight(height + 1);
		}
	}
	
	public async getBlockByHash(hash: Buffer): Promise<Buffer | null> {
		const blockHeader = await this.getBlockHeader(hash);
		if(blockHeader === null) {
			return null;
		}
		const txHashes = await this.getBlockTransactionHashes(hash);
		if(txHashes === null) {
			return null;
		}
		const txs = await Promise.all(txHashes.map(async (txHash) => {
			const tx = await this.getTransaction(txHash);
			if(tx === null) {
				throw new Error('Transaction not found');
			}
			return tx;
		}));
		const txLengthBuffer = varuint.encode(txs.length);
		const block = Buffer.concat([blockHeader, txLengthBuffer, ...txs]);
		return block;
	}
	
}

