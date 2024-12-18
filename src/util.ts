
import fs from 'fs';
import path from 'path';

import { parse } from 'yaml';

export interface ServerConfig {
	port?: number;
	host?: string;
}

export interface ChainConfig {
	restUrl: string;
	server?: ServerConfig;
}

export interface Config {
	redisUrl: string;
	chains: { [chain: string]: ChainConfig };
}

export const loadConfig = (_path: string = path.resolve(__dirname, '../../config.yaml')): Config => {
	const config = parse(fs.readFileSync(_path, 'utf8'));
	return config;
};

