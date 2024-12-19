
import { setTimeout } from 'timers/promises';

import { loadConfig } from '../../src/util';
import { BlockDownloader } from '../../src/BlockDownloader';

export const main = async () => {
	if(process.argv.length < 3) {
		console.log('Usage: node blockDownloader.js <chainName>');
		process.exit(1);
	}
	const chainName = process.argv[2];
	const config = loadConfig();
	const chainConfig = config.chains[chainName];
	if(!chainConfig) {
		console.log(`Chain not found: ${chainName}`);
		process.exit(1);
	}
	const downloader = new BlockDownloader(chainConfig.restUrl);
	downloader.run(0);
	for(;;) {
		console.log(`Running: ${downloader.runningCount}, Completed: ${downloader.completedCount}`);
		await setTimeout(1000);
	}
};

main();

