#!/bin/sh
set -eu
if [ -n "${QUICKDPI_KEYSTORE_B64:-}" ]; then
    printf '%s' "$QUICKDPI_KEYSTORE_B64" | base64 -d > /tmp/quickdpi-release.jks
    export QUICKDPI_KEYSTORE=/tmp/quickdpi-release.jks
fi
cd android
./gradlew --no-daemon assembleRelease
cd ..
rm -f /tmp/quickdpi-release.jks
mkdir -p dist
cp android/app/build/outputs/apk/release/app-release.apk dist/quickdpi-android.apk
