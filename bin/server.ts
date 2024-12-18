
import express from 'express';
import {
	Transaction,
} from 'bitcoinjs-lib';

import { loadConfig } from '../src/util';
import { Client } from '../src/Client';
import { getExpressApp } from '../src/server';

export const main = async () => {
	if(process.argv.length < 3) {
		console.log('Usage: node server.js <chainName>');
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
	const client = new Client(config.redisUrl);
	const app = getExpressApp(client);
	// Listen.
	const port = chainConfig.server?.port || 8000;
	const host = chainConfig.server?.host || 'localhost';
	app.listen(port, host, () => {
		console.log(`Server started on ${port}.`);
	});
};

main();

