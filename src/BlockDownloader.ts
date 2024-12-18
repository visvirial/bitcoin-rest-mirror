
import { setTimeout } from 'timers/promises';

require('dotenv').config();

export type BlockStatus = 'running' | 'completed' | 'failed';

export interface BlockData {
	status: BlockStatus;
	blockPromise: Promise<Buffer | null>;
}

export class BlockDownloader {
	
	public maxBlocks: number = 1000;
	public concurrency: number = 32;
	
	private _recentHeight: number = 0;
	private _nextHeight: number = 0;
	private _blocks: Map<number, BlockData> = new Map();
	
	constructor() {
	}
	
	public async shiftBlock() {
		const height = this._recentHeight;
		const blockData = await (async () => {
			for(;;) {
				const blockData = this._blocks.get(height);
				if(!blockData) {
					//throw new Error('Block not fetched yet.');
					await setTimeout(100);
					continue;
				}
				return blockData;
			}
		})();
		this._blocks.delete(height);
		this._recentHeight++;
		return {
			height,
			block: await blockData.blockPromise,
		};
	}
	
	private _getStatusCount(status: BlockStatus) {
		let count = 0;
		for(const [height, blockData] of this._blocks) {
			if(blockData.status === status) {
				count++;
			}
		}
		return count;
	}
	
	public get nextHeight() {
		return this._nextHeight;
	}
	
	public get blockCount() {
		return this._blocks.size;
	}
	
	public get runningCount() {
		return this._getStatusCount('running');
	}
	
	public get completedCount() {
		return this._getStatusCount('completed');
	}
	
	public get failedCount() {
		return this._getStatusCount('failed');
	}
	
	public static async fetchBlockByHeight(height: number): Promise<Buffer | null> {
		const maxRetry = 5;
		for(let i=0; i<maxRetry; i++) {
			const blockHashBuffer = Buffer.from(await (await fetch(
				`${process.env.BITCOIN_REST_URL}/blockhashbyheight/${height}.bin`
			)).arrayBuffer());
			if(blockHashBuffer.length !== 32) {
				if(blockHashBuffer.toString('utf-8') === 'Block height out of range') {
					return null;
				}
				console.log(`Failed to fetch block hash: ${height}`);
				continue;
			}
			const blockHash = blockHashBuffer.reverse().toString('hex');
			const blockBuffer = Buffer.from(await (await fetch(
				`${process.env.BITCOIN_REST_URL}/block/${blockHash}.bin`
			)).arrayBuffer());
			if(blockBuffer.length <= 80) {
				console.log('Block length invalid:', blockBuffer.toString('utf-8'));
				await setTimeout(1000);
				continue;
			}
			return blockBuffer;
		}
		throw new Error('Failed to fetch block: max retry count reached.');
	}
	
	public async run(startingHeight: number) {
		this._recentHeight = startingHeight;
		this._nextHeight = startingHeight;
		// Fetch blocks.
		(async () => {
			for(;;) {
				//console.log(this.blockCount, this.runningCount, this.completedCount, this.maxBlocks);
				if(this.blockCount >= this.maxBlocks || this.runningCount >= this.concurrency) {
					await setTimeout(100);
					continue;
				}
				for(let i=this.runningCount; i<this.concurrency; i++) {
					const nextHeight = this._nextHeight;
					const blockPromise = BlockDownloader.fetchBlockByHeight(nextHeight);
					blockPromise.then((_) => {
						const block = this._blocks.get(nextHeight);
						if(block) {
							block.status = block ? 'completed' : 'failed';
						}
					});
					const blockData = {
						status: 'running' as BlockStatus,
						blockPromise,
					};
					this._blocks.set(this._nextHeight, blockData);
					this._nextHeight++;
					//await setTimeout(10);
				}
			}
		})();
	}
	
}

