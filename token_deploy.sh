#!/bin/bash
set -e

ACCOUNT_ID=token.kenhorizon.testnet

./build_token.sh;

near deploy $ACCOUNT_ID ./res/token.wasm
