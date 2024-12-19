
import express from 'express';
import morgan from 'morgan';
import {
	Transaction,
} from 'bitcoinjs-lib';

import { Client } from '../src/Client';

export const respond = (req: express.Request, res: express.Response, data: Buffer, toObject?: (data: Buffer) => any) => {
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
			if(!toObject) {
				res.status(404).send({ error: 'Not implemented.' });
				break;
			}
			res.send(toObject(data));
			break;
		default:
			res.status(400).send(`Invalid extension: ${req.params.ext}`);
			break;
	}
}

/*
export const transactionToObject = (txBuffer: Buffer) => {
	throw new Error('Not implemented');
};
*/

export const getExpressApp = (client: Client) => {
	const app = express();
	//app.use(morgan('combined'));
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
		respond(req, res, txBuffer);
	});
	app.get('/rest/block/:blockId.:ext', async (req, res) => {
		const blockId = Buffer.from(req.params.blockId, 'hex');
		if(blockId.length !== 32) {
			res.status(400).send(`Invalid hash: ${req.params.blockId}`);
			return;
		}
		const blockBuffer = await client.getBlockByHash(blockId.reverse());
		if(!blockBuffer) {
			res.status(404).send(`${req.params.blockId} not found`);
			return;
		}
		respond(req, res, blockBuffer);
	});
	app.get('/rest/headers/:blockId.:ext', async (req, res) => {
		const blockId = Buffer.from(req.params.blockId, 'hex');
		const count = req.query.count ? +req.query.count : 5;
		if(blockId.length !== 32) {
			res.status(400).send(`Invalid hash: ${req.params.blockId}`);
			return;
		}
		const height = await client.getBlockHeightByHash(blockId.reverse());
		if(height === null) {
			res.status(404).send(`${req.params.blockId} not found`);
			return;
		}
		const blockHeaders: Buffer[] = [];
		for(let _height=height; _height<height+count; _height++) {
			const blockHash = await client.getBlockHashByHeight(_height);
			if(!blockHash) {
				break;
			}
			const blockHeader = await client.getBlockHeader(blockHash);
			if(!blockHeader) {
				break;
			}
			blockHeaders.push(blockHeader);
		}
		respond(req, res, Buffer.concat(blockHeaders));
	});
	app.get('/rest/blockhashbyheight/:height.:ext', async (req, res) => {
		if(!req.params.height.match(/^\d+$/)) {
			res.status(400).send(`Invalid height: ${req.params.height}`);
			return;
		}
		const height = +req.params.height;
		const blockHash = await client.getBlockHashByHeight(height);
		if(blockHash === null) {
			res.status(404).send(`${req.params.height} not found`);
			return;
		}
		respond(req, res, blockHash.reverse());
	});
	// Handle 404.
	app.all('*', (req, res) => {
		res.type('text/html');
		res.status(404).send('');
	});
	return app;
};

