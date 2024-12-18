
import express from 'express';
import {
	Transaction,
} from 'bitcoinjs-lib';

import { Client } from '../src/Client';

export const respond = (res: express.Response, data: Buffer, toObject: (data: Buffer) => any) => {
	switch(req.params.ext) {
		case 'bin':
			res.type('application/octet-stream');
			res.send(data);
			break;
		case 'hex':
			res.type('text/plain');
			res.send(data.toString('hex'));
			break;
		case 'json':
			res.type('application/json');
			res.send(toObject(data));
			break;
		default:
			res.status(400).send(`Invalid extension: ${req.params.ext}`);
			break;
	}
}

export const transactionToObject = (txBuffer: Buffer) => {
	throw new Error('Not implemented');
};

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
		respond(res, txBuffer, transactionToObject);
	});
	app.all('*', (req, res) => {
		res.type('text/html');
		res.status(404).send('');
	});
	return app;
};

