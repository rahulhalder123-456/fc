#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo fmt --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
mkdir -p dist
cp target/release/fcz dist/fcz-linux-x86_64
chmod 755 dist/fcz-linux-x86_64
(cd dist && sha256sum fcz-linux-x86_64 > SHA256SUMS)
./dist/fcz-linux-x86_64 --version
echo "Assets created under $(pwd)/dist. Nothing was uploaded."
