
import fs from 'fs';
import path from 'path';
import crypto from 'crypto';

import {
	Block,
} from 'bitcoinjs-lib';

import { Client } from './Client';

jest.mock('ioredis', () => require('ioredis-mock'));

describe('Client', () => {
	// Load blocks.
	const blocks: Buffer[] = [];
	for(let height=0; height<=1000; height++) {
		const blockBuffer = fs.readFileSync(path.resolve(__dirname, `../test/fixtures/block_${height}.bin`));
		blocks.push(blockBuffer);
	}
	describe('Low-level operations', () => {
		describe('next block height', () => {
			test('getNextBlockHeight (first)', async () => {
				const client = new Client('');
				const height = await client.getNextBlockHeight();
				expect(height).toBe(0);
				await client.destroy();
			});
			test('setNextBlockHeight', async () => {
				const client = new Client('');
				await client.setNextBlockHeight(12345);
				const height = await client.getNextBlockHeight();
				expect(height).toBe(12345);
				await client.destroy();
			});
		});
		describe('block header', () => {
			test('getBlockHeader (none)', async () => {
				const client = new Client('');
				const blockHash = Buffer.alloc(32);
				const header = await client.getBlockHeader(blockHash);
				expect(header).toBe(null);
				await client.destroy();
			});
			test('setBlockHeader', async () => {
				const client = new Client('');
				const block = Block.fromBuffer(blocks[0]);
				const blockHash = block.getHash();
				await client.setBlockHeader(blockHash, block.toBuffer(true));
				const header = await client.getBlockHeader(blockHash);
				expect(header!.toString('hex')).toBe(block.toBuffer(true).toString('hex'));
				await client.destroy();
			});
		});
		describe('block hash by height', () => {
			test('getBlockHashByHeight (none)', async () => {
				const client = new Client('');
				const blockHash = await client.getBlockHashByHeight(0);
				expect(blockHash).toBe(null);
				await client.destroy();
			});
			test('setBlockHashByHeight', async () => {
				const client = new Client('');
				const block = Block.fromBuffer(blocks[0]);
				const blockHash = block.getHash();
				await client.setBlockHashByHeight(0, blockHash);
				const blockHashActual = await client.getBlockHashByHeight(0);
				expect(blockHashActual!.toString('hex')).toBe(blockHash.toString('hex'));
				await client.destroy();
			});
		});
		describe('block height by hash', () => {
			test('getBlockHeightByHash (none)', async () => {
				const client = new Client('');
				const blockHash = crypto.randomBytes(32);
				const height = await client.getBlockHeightByHash(blockHash);
				expect(height).toBe(null);
				await client.destroy();
			});
			test('setBlockHeightByHash', async () => {
				const client = new Client('');
				const block = Block.fromBuffer(blocks[0]);
				const blockHash = block.getHash();
				await client.setBlockHeightByHash(0, blockHash);
				const height = await client.getBlockHeightByHash(blockHash);
				expect(height).toBe(0);
				await client.destroy();
			});
		});
		describe('block transaction hashes', () => {
			test('getBlockTransactionHashes (none)', async () => {
				const client = new Client('');
				const blockHash = Buffer.alloc(32);
				const txHashes = await client.getBlockTransactionHashes(blockHash);
				expect(txHashes).toBe(null);
				await client.destroy();
			});
			test('setBlockTransactionHashes', async () => {
				const client = new Client('');
				const block = Block.fromBuffer(blocks[0]);
				const blockHash = block.getHash();
				await client.setBlockTransactionHashes(blockHash, block.transactions!.map(tx => tx.getHash()));
				const txHashes = await client.getBlockTransactionHashes(blockHash);
				expect(txHashes!.map(txHash => txHash.toString('hex'))).toStrictEqual(block.transactions!.map(tx => tx.getHash().toString('hex')));
				await client.destroy();
			});
		});
		describe('transaction', () => {
			test('getTransaction (none)', async () => {
				const client = new Client('');
				const txHash = Buffer.alloc(32);
				const tx = await client.getTransaction(txHash);
				expect(tx).toBe(null);
				await client.destroy();
			});
			test('setTransaction', async () => {
				const client = new Client('');
				const block = Block.fromBuffer(blocks[0]);
				const tx = block.transactions![0];
				const txHash = tx.getHash();
				await client.setTransaction(txHash, tx.toBuffer());
				const txActual = await client.getTransaction(txHash);
				expect(txActual!.toString('hex')).toBe(tx.toBuffer().toString('hex'));
				await client.destroy();
			});
		});
	});
	describe('High-level operations', () => {
		test('addBlock', async () => {
			const client = new Client('');
			for(let height=0; height<=1000; height++) {
				const block = Block.fromBuffer(blocks[height]);
				const blockHash = block.getHash();
				await client.addBlock(height, blocks[height]);
			}
			for(let height=0; height<=1000; height++) {
				const block = Block.fromBuffer(blocks[height]);
				const blockHash = block.getHash();
				const blockActual = await client.getBlockByHash(blockHash);
				expect(blockActual!.toString('hex')).toBe(blocks[height].toString('hex'));
			}
			await client.destroy();
		});
	});
});

