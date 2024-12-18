// vim: set noexpandtab:

import { Syncer } from '../src/Syncer';
import { Client } from '../src/Client';

require('dotenv').config();

export const main = async () => {
	const client = new Client(process.env.REDIS_URL!);
	const syncer = new Syncer(client);
	await syncer.run();
};

main();

