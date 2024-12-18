// vim: set noexpandtab:

import { loadConfig } from '../src/util';
import { BlockDownloader } from '../src/BlockDownloader';
import { Syncer } from '../src/Syncer';
import { Client } from '../src/Client';

export const main = async () => {
	if(process.argv.length < 3) {
		console.log('Usage: node syncer.js <chainName>');
		process.exit(1);
	}
	const chainName = process.argv[2];
	const config = loadConfig();
	const chainConfig = config.chains[chainName];
	if(!chainConfig) {
		console.log(`Chain not found: ${chainName}`);
		process.exit(1);
	}
	// Initialize client.
	const downloader = new BlockDownloader(chainConfig.restUrl);
	const client = new Client(config.redisUrl, chainName);
	const syncer = new Syncer(downloader, client);
	await syncer.run();
};

main();

