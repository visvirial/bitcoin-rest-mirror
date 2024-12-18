#!/bin/bash

BITCOIN_CLI="bitcoin-cli"

mkdir -p fixtures

for blockHeight in $(seq 0 1000); do
	echo Downloading block \#${blockHeight}...
	blockHash=$($BITCOIN_CLI getblockhash $blockHeight)
	echo $blockHash
	$BITCOIN_CLI getblock $blockHash 0 | xxd -r -p >fixtures/block_${blockHeight}.bin
done

