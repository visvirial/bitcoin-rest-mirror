
import request from 'supertest';

import { Client } from './Client';
import { getExpressApp } from './server';

jest.mock('ioredis', () => require('ioredis-mock'));

describe('Server', () => {
	const client = new Client('');
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
	});
});

