#!/bin/sh
set -eu
PACKAGES=""
. ci/toolchain.sh
cargo install cargo-deb --locked
cargo deb
mkdir -p dist
cp target/debian/*.deb dist/quickdpi_amd64.deb
tar -czf dist/quickdpi-linux-x86_64.tar.gz -C target/release quickdpi
