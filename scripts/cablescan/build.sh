#!/bin/bash
# Build minimal BLE scanner APK. Targets SDK 28 so BLUETOOTH_SCAN
# is auto-mapped from legacy BLUETOOTH on Android 12+ via
# usesPermissionFlags="neverForLocation" (no ACCESS_FINE_LOCATION
# required). At runtime we still call requestPermissions() so
# Android 12+ grants it via the standard dialog.
#
# Usage: bash scripts/cablescan/build.sh
#
# Output: scripts/cablescan/build/cablescan.apk
# Install: adb install -t -r -d --bypass-low-target-sdk-block \
#              scripts/cablescan/build/cablescan.apk

set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
SDK="${ANDROID_HOME:-/home/mmacedoeu/Android/Sdk}"
BT=$SDK/build-tools/34.0.0
PLATFORM_JAR=$SDK/platforms/android-34/android.jar

cd "$ROOT"
rm -rf build && mkdir -p build/classes

echo "[1] compile"
javac -source 1.8 -target 1.8 -bootclasspath "$PLATFORM_JAR" \
      -d build/classes \
      src/com/cablescan/MainActivity.java

echo "[2] dex"
"$BT/d8" --output build/ build/classes/com/cablescan/*.class

echo "[3] link"
"$BT/aapt2" link \
  -o build/resources.apk \
  -I "$PLATFORM_JAR" \
  --manifest AndroidManifest.xml \
  --target-sdk-version 28 \
  --min-sdk-version 21 \
  --version-code 2 \
  --version-name 0.2

echo "[4] inject classes.dex"
( cd build && zip -j resources.apk classes.dex )
mv build/resources.apk build/app.raw.apk

echo "[5] zipalign"
"$BT/zipalign" -p -f 4 build/app.raw.apk build/app.aligned.apk

echo "[6] sign"
KEYSTORE="$HOME/.android/debug.keystore"
if [ ! -f "$KEYSTORE" ]; then
    mkdir -p "$(dirname "$KEYSTORE")"
    keytool -genkey -v -keystore "$KEYSTORE" \
            -alias androiddebugkey -keyalg RSA -keysize 2048 \
            -validity 10000 \
            -storepass android -keypass android \
            -dname "CN=Android Debug,O=Android,C=US" 2>&1 | tail -3
fi
"$BT/apksigner" sign --ks "$KEYSTORE" --ks-pass pass:android --key-pass pass:android \
    --out build/cablescan.apk \
    build/app.aligned.apk

"$BT/apksigner" verify build/cablescan.apk && echo "[verify ok]"

ls -la build/cablescan.apk
echo "[done] apk at $ROOT/build/cablescan.apk"
echo "install: adb install -t -r -d --bypass-low-target-sdk-block $ROOT/build/cablescan.apk"
