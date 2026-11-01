#!/usr/bin/env bash
# Builds target/android/last-harvest.apk for arm64 phones: the game as a NativeActivity library, with
# game_data.pak as an asset, signed with the local debug key. Needs the Android SDK and NDK, JDK 17 and cargo-ndk.
# --install also installs it on the phone adb sees.
set -euo pipefail
cd "$(dirname "$0")/.."
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)}"
if [ -z "${JAVA_HOME:-}" ] && [ -d /opt/homebrew/opt/openjdk@17 ]; then
  export JAVA_HOME=/opt/homebrew/opt/openjdk@17
fi
tools="$(ls -d "$ANDROID_HOME"/build-tools/* | sort -V | tail -1)"
jar="$ANDROID_HOME/platforms/android-36/android.jar"
out=target/android
stage="$out/stage"
rm -rf "$stage"
mkdir -p "$stage/assets" "$stage/lib/arm64-v8a" "$stage/res/mipmap"

cargo run -q -p last-harvest-pack
CARGO_PROFILE_RELEASE_STRIP=symbols CARGO_PROFILE_RELEASE_DEBUG=false cargo ndk -t arm64-v8a --platform 26 -- rustc --release -p last-harvest --lib --crate-type cdylib --no-default-features
cp target/aarch64-linux-android/release/liblast_harvest.so "$stage/lib/arm64-v8a/"
cp target/game_data.pak "$stage/assets/"
cp crates/editor/web/icon.png "$stage/res/mipmap/icon.png"
sed "s/@VERSION@/${LAST_HARVEST_VERSION:-dev}/" crates/app/android/AndroidManifest.xml > "$stage/AndroidManifest.xml"

"$tools/aapt2" compile --dir "$stage/res" -o "$stage/res.zip"
"$tools/aapt2" link -o "$stage/unsigned.apk" --manifest "$stage/AndroidManifest.xml" -I "$jar" -R "$stage/res.zip" -A "$stage/assets"
(cd "$stage" && zip -q -r unsigned.apk lib)
"$tools/zipalign" -f -P 16 4 "$stage/unsigned.apk" "$stage/aligned.apk"
key="$HOME/.android/debug.keystore"
if [ ! -f "$key" ]; then
  mkdir -p "$(dirname "$key")"
  "$JAVA_HOME/bin/keytool" -genkeypair -keystore "$key" -storepass android -keypass android -alias androiddebugkey \
    -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US"
fi
"$tools/apksigner" sign --ks "$key" --ks-pass pass:android --key-pass pass:android --ks-key-alias androiddebugkey \
  --out "$out/last-harvest.apk" "$stage/aligned.apk"
echo "built $out/last-harvest.apk"
if [ "${1:-}" = --install ]; then
  adb install -r "$out/last-harvest.apk"
fi
