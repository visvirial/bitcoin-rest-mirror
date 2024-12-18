
import os from 'os';
import path from 'path';
import { setTimeout } from 'timers/promises';
import { Worker } from 'worker_threads';

import { Mutex } from 'await-semaphore';

import { BlockDownloader } from './BlockDownloader';
import { Client } from './Client';

export class Syncer {
	
	private _workers: Worker[] = [];
	private _currentWorkerIndex = 0;
	
	private _nextBlockHeight: number = 0;
	private _acceptedBlockHeights: Set<number> = new Set();
	private _jobCount = 0;
	
	constructor(
		private _downloader: BlockDownloader,
		private _client: Client,
	) {
	}
	
	private get worker() {
		return this._workers[this._currentWorkerIndex];
	}
	
	private incrementWorkerIndex() {
		this._currentWorkerIndex = (this._currentWorkerIndex + 1) % this._workers.length;
	}
	
	public async run() {
		// Print stat.
		(async () => {
			for(;;) {
				await setTimeout(1000);
				console.log(`Processing: #${this._nextBlockHeight.toLocaleString()}, Downloaded: ${this._downloader.completedCount.toLocaleString()}, Running: ${this._downloader.runningCount.toLocaleString()}, Total: ${this._downloader.blockCount.toLocaleString()}`);
			}
		})();
		// Get next block height.
		this._nextBlockHeight = await this._client.getNextBlockHeight();
		// Run block downloader.
		await this._downloader.run(this._nextBlockHeight);
		// Launch workers.
		const workerCount = os.cpus().length;
		const mutex = new Mutex();
		for(let i=0; i<workerCount; i++) {
			this._workers.push(new Worker(
				path.resolve(__dirname, './worker.syncer.js'),
				{
					workerData: {
						redisUrl: this._client.redisUrl,
						chain: this._client.chain,
						prefix: this._client.prefix,
					},
				}
			));
			this._workers[i].on('message', async (data) => {
				this._jobCount--;
				const release = await mutex.acquire();
				if(data.type === 'acceptBlock') {
					const { height } = data.payload;
					this._acceptedBlockHeights.add(height);
					for(;;) {
						if(this._acceptedBlockHeights.has(this._nextBlockHeight)) {
							this._client.setNextBlockHeight(this._nextBlockHeight + 1);
							this._acceptedBlockHeights.delete(this._nextBlockHeight);
							this._nextBlockHeight++;
						} else {
							break;
						}
					}
				}
				release();
			});
		}
		// Do initial sync.
		for(;;) {
			if(this._jobCount >= 5 * workerCount) {
				await setTimeout(100);
				continue;
			}
			const { height, block } = await this._downloader.shiftBlock();
			// No more blocks.
			if(!block) {
				break;
			}
			// Send block to worker.
			this.worker.postMessage({ type: 'block', payload: { height, block } });
			this.incrementWorkerIndex();
			this._jobCount++;
			//await this._client.acceptBlock(height, block);
		}
	}
	
}

