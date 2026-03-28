#!/bin/sh

set -eo pipefail

cd ../surfer
git submodule update --init --recursive
trunk build --release
cd -
mkdir -p extension/surfer
cp ../surfer/dist/manifest.json \
  ../surfer/dist/index.html \
  ../surfer/dist/surfer.js \
  ../surfer/dist/surfer_bg.wasm \
  ../surfer/dist/sw.js \
  ../surfer/dist/integration.js \
  extension/surfer

cp README.md extension/README.md

python3 prepare.py
