
import fs from 'fs';
import path from 'path';

import request from 'supertest';
import {
	Block,
} from 'bitcoinjs-lib';

import { Client } from './Client';
import { getExpressApp } from './server';

jest.mock('ioredis', () => require('ioredis-mock'));

describe('Server', () => {
	// Load blocks.
	const blocks: Buffer[] = [];
	for(let height=0; height<=1000; height++) {
		const blockBuffer = fs.readFileSync(path.resolve(__dirname, `../test/fixtures/block_${height}.bin`));
		blocks.push(blockBuffer);
	}
	// Initialize client.
	const client = new Client('');
	// Add blocks.
	beforeAll(async () => {
		for(let height=0; height<=1000; height++) {
			await client.addBlock(height, blocks[height]);
		}
	});
	// Get express app.
	const app = getExpressApp(client);
	test('GET /rest', async () => {
		const res = await request(app).get('/rest');
		expect(res.status).toBe(404);
		expect(res.text).toBe('');
	});
	describe('GET /rest/tx', () => {
		test('Invalid tx hash', async () => {
			const res = await request(app).get('/rest/tx/invalid.hex');
			expect(res.status).toBe(400);
			expect(res.text).toBe('Invalid hash: invalid');
		});
		test('Get as bin', async () => {
			const tx = Block.fromBuffer(blocks[0]).transactions![0];
			const txId = tx.getId();
			const res = await request(app).get(`/rest/tx/${txId}.bin`).responseType('blob');
			expect(res.status).toBe(200);
			expect(res.body).toStrictEqual(tx.toBuffer());
		});
		test('Get as hex', async () => {
			const tx = Block.fromBuffer(blocks[0]).transactions![0];
			const txId = tx.getId();
			const res = await request(app).get(`/rest/tx/${txId}.hex`);
			expect(res.status).toBe(200);
			expect(res.text).toBe(tx.toBuffer().toString('hex'));
		});
	});
	describe('GET /rest/block', () => {
		test('Invalid block hash', async () => {
			const res = await request(app).get('/rest/block/invalid.hex');
			expect(res.status).toBe(400);
			expect(res.text).toBe('Invalid hash: invalid');
		});
		test('Get as bin', async () => {
			const blockBuffer = blocks[0];
			const block = Block.fromBuffer(blockBuffer);
			const blockId = block.getId();
			const res = await request(app).get(`/rest/block/${blockId}.bin`).responseType('blob');
			expect(res.status).toBe(200);
			expect(res.body).toStrictEqual(blockBuffer);
		});
		test('Get as hex', async () => {
			const blockBuffer = blocks[0];
			const block = Block.fromBuffer(blockBuffer);
			const blockId = block.getId();
			const res = await request(app).get(`/rest/block/${blockId}.hex`);
			expect(res.status).toBe(200);
			expect(res.text).toBe(blockBuffer.toString('hex'));
		});
	});
	describe('GET /rest/headers', () => {
		test('Invalid block hash', async () => {
			const res = await request(app).get('/rest/headers/invalid.hex?count=10');
			expect(res.status).toBe(400);
			expect(res.text).toBe('Invalid hash: invalid');
		});
		test('Get as bin (count=default)', async () => {
			const blockBuffer = blocks[0];
			const block = Block.fromBuffer(blockBuffer);
			const blockId = block.getId();
			const res = await request(app).get(`/rest/headers/${blockId}.bin`).responseType('blob');
			expect(res.status).toBe(200);
			expect(res.body).toStrictEqual(Buffer.concat(blocks.slice(0, 5).map(b => Block.fromBuffer(b).toBuffer(true))));
		});
		test('Get as bin (count=10)', async () => {
			const blockBuffer = blocks[0];
			const block = Block.fromBuffer(blockBuffer);
			const blockId = block.getId();
			const res = await request(app).get(`/rest/headers/${blockId}.bin?count=10`).responseType('blob');
			expect(res.status).toBe(200);
			expect(res.body).toStrictEqual(Buffer.concat(blocks.slice(0, 10).map(b => Block.fromBuffer(b).toBuffer(true))));
		});
		test('Get as hex', async () => {
			const blockBuffer = blocks[0];
			const block = Block.fromBuffer(blockBuffer);
			const blockId = block.getId();
			const res = await request(app).get(`/rest/headers/${blockId}.hex?count=10`);
			expect(res.status).toBe(200);
			expect(res.text).toBe(Buffer.concat(blocks.slice(0, 10).map(b => Block.fromBuffer(b).toBuffer(true))).toString('hex'));
		});
	});
	describe('GET /rest/blockhashbyheight', () => {
		test('Invalid block hash', async () => {
			const res = await request(app).get('/rest/blockhashbyheight/invalid.hex');
			expect(res.status).toBe(400);
			expect(res.text).toBe('Invalid height: invalid');
		});
		test('Get as bin', async () => {
			const blockBuffer = blocks[0];
			const block = Block.fromBuffer(blockBuffer);
			const res = await request(app).get(`/rest/blockhashbyheight/0.bin`).responseType('blob');
			expect(res.status).toBe(200);
			expect(res.body).toStrictEqual(block.getHash().reverse());
		});
		test('Get as hex', async () => {
			const blockBuffer = blocks[0];
			const block = Block.fromBuffer(blockBuffer);
			const res = await request(app).get(`/rest/blockhashbyheight/0.hex`);
			expect(res.status).toBe(200);
			expect(res.text).toBe(block.getHash().reverse().toString('hex'));
		});
	});
});

