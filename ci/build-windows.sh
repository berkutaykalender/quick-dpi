#!/bin/sh
set -eu
PACKAGES="gcc-mingw-w64-x86-64 binutils-mingw-w64-x86-64"
. ci/toolchain.sh
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
mkdir -p dist
cp target/x86_64-pc-windows-gnu/release/quickdpi.exe dist/quickdpi-windows-x86_64.exe
