
import express from 'express';
import {
	Transaction,
} from 'bitcoinjs-lib';

import { Client } from '../src/Client';
import { getExpressApp } from '../src/server';

require('dotenv').config();

export const main = async () => {
	// Initialize client.
	const client = new Client(process.env.REDIS_URL!);
	const app = getExpressApp(client);
	// Listen.
	const port = process.env.HTTP_PORT || 8000;
	app.listen(port, () => {
		console.log(`Server started on ${port}.`);
	});
};

main();

