
import express from 'express';
import {
	Transaction,
} from 'bitcoinjs-lib';

import { Client } from '../src/Client';

export const getExpressApp = (client: Client) => {
	const app = express();
	// Register routes.
	app.get('/rest/tx/:txId.:ext', async (req, res) => {
		const txId = Buffer.from(req.params.txId, 'hex');
		if(txId.length !== 32) {
			res.status(400).send(`Invalid hash: ${req.params.txId}`);
			return;
		}
		const txBuffer = await client.getTransaction(txId.reverse());
		if(!txBuffer) {
			res.status(404).send(`${req.params.txId} not found`);
			return;
		}
		switch(req.params.ext) {
			case 'bin':
				res.type('application/octet-stream');
				res.send(txBuffer);
				break;
			case 'hex':
				res.type('text/plain');
				res.send(txBuffer.toString('hex'));
				break;
			case 'json':
				res.type('application/json');
				const tx = Transaction.fromBuffer(txBuffer);
				res.send(JSON.stringify(tx));
				break;
			default:
				res.status(400).send(`Invalid extension: ${req.params.ext}`);
				break;
		}
	});
	app.all('*', (req, res) => {
		res.type('text/html');
		res.status(404).send('');
	});
	return app;
};

