
import { setTimeout } from 'timers/promises';

import { loadConfig } from '../../src/util';
import { BlockDownloader } from '../../src/BlockDownloader';

export const main = async () => {
	if(process.argv.length < 3) {
		console.log('Usage: node blockDownloader.js <chainName> [<startHeight>]');
		process.exit(1);
	}
	const chainName = process.argv[2];
	const startHeight = process.argv[3] ? parseInt(process.argv[3]) : 0;
	const config = loadConfig();
	const chainConfig = config.chains[chainName];
	if(!chainConfig) {
		console.log(`Chain not found: ${chainName}`);
		process.exit(1);
	}
	const downloader = new BlockDownloader(chainConfig.restUrl);
	downloader.run(startHeight);
	let lastLapTime = Date.now();
	let completed = 0;
	let lastCompleted = 0;
	for(;;) {
		const { height, block } = await downloader.shiftBlock();
		if(!block) {
			console.log(`No more blocks.`);
			break;
		}
		completed++;
		lastCompleted++;
		if(Date.now() - lastLapTime > 1000) {
			const blocksPerSecond = lastCompleted / ((Date.now() - lastLapTime) / 1000);
			console.log(`Completed: ${completed.toLocaleString()}, ${blocksPerSecond.toFixed(2)} blocks/s`);
			lastLapTime = Date.now();
			lastCompleted = 0;
		}
	}
};

main();

