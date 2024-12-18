
import { setTimeout } from 'timers/promises';

import { BlockDownloader } from '../../src/BlockDownloader';

export const main = async () => {
	const downloader = new BlockDownloader();
	downloader.run(0);
	for(;;) {
		console.log(`Running: ${downloader.runningCount}, Completed: ${downloader.completedCount}, Failed: ${downloader.failedCount}`);
		await setTimeout(1000);
	}
};

main();

