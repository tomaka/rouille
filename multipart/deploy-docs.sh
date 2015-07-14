#!/bin/sh

cargo doc -v --no-deps
cp -R target/doc doc
curl http://www.rust-ci.org/artifacts/put?t=$RUSTCI_TOKEN | sh
rm -r doc
