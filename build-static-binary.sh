#!/usr/bin/env bash

# the musl-tools package must be installed.
rustup target add x86_64-unknown-linux-musl
cargo build --release --target=x86_64-unknown-linux-musl
cp target/x86_64-unknown-linux-musl/release/tantivy  ../../github/fulmicoton.github.io/tantivy-files/binaries/linux_x86_64/