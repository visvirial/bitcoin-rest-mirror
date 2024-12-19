
import {
	workerData,
	parentPort,
} from 'worker_threads';

import { Client } from './Client';

export const main = async () => {
	if(!parentPort) {
		throw new Error('Do not invoke this script directly!');
	}
	const client = new Client(workerData.redisUrl, workerData.chain, workerData.prefix);
	parentPort!.on('message', async (data) => {
		if(data.type === 'block') {
			const { height, block } = data.payload;
			await client.addBlock(height, Buffer.from(block), false);
			parentPort!.postMessage({ type: 'addBlock', payload: { height } });
		}
	});
};

main();

