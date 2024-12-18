
import { BlockDownloader } from './BlockDownloader';
import { Client } from './Client';

export class Syncer {
	
	private _downloader: BlockDownloader = new BlockDownloader();
	
	constructor(
		private _client: Client,
	) {
	}
	
	async run() {
		// Get next block height.
		const nextBlockHeight = await this._client.getNextBlockHeight();
		// Do initial sync.
		await this._downloader.run(nextBlockHeight);
		for(;;) {
			const { height, block } = await this._downloader.shiftBlock();
			if(!block) {
				break;
			}
			await this._client.acceptBlock(height, block);
			await this._client.setNextBlockHeight(height + 1);
			console.log(`Block #${height.toLocaleString()} processed.`);
		}
	}
	
}

