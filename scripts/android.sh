#!/usr/bin/env bash
# Builds target/android/last-harvest.apk for arm64 phones: the game as a NativeActivity library, MainActivity.java
# for the vibrator and a second screen, and game_data.pak as an asset. Needs the Android SDK and NDK, JDK 17 and
# cargo-ndk. --install also installs it on the phone adb sees.
#
# A release build sets LAST_HARVEST_VERSION (its tag, which also gives the versionCode) and LAST_HARVEST_NO_PAK=1:
# the APK then fetches the release's shared game_data.pak on first run. ANDROID_KEYSTORE, ANDROID_KEYSTORE_PASSWORD
# and ANDROID_KEY_ALIAS sign it; without them it signs with the local debug key.
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
mkdir -p "$stage/assets" "$stage/lib/arm64-v8a" "$stage/res/mipmap" "$stage/classes"

cargo run -q -p last-harvest-pack
CARGO_PROFILE_RELEASE_STRIP=symbols CARGO_PROFILE_RELEASE_DEBUG=false cargo ndk -t arm64-v8a --platform 26 -- rustc --release -p last-harvest --lib --crate-type cdylib --no-default-features
cp target/aarch64-linux-android/release/liblast_harvest.so "$stage/lib/arm64-v8a/"
[ "${LAST_HARVEST_NO_PAK:-}" = 1 ] || cp target/game_data.pak "$stage/assets/"
cp crates/editor/web/icon.png "$stage/res/mipmap/icon.png"
version="${LAST_HARVEST_VERSION:-dev}"
# v1.2.3 is 10203, so each release installs over the one before; dev builds are 1.
code=1
if [[ "$version" =~ ^v?([0-9]+)\.([0-9]+)\.([0-9]+) ]]; then
  code=$((BASH_REMATCH[1] * 10000 + BASH_REMATCH[2] * 100 + BASH_REMATCH[3]))
fi
sed -e "s/@VERSION@/$version/" -e "s/@CODE@/$code/" crates/app/android/AndroidManifest.xml > "$stage/AndroidManifest.xml"

"$tools/aapt2" compile --dir "$stage/res" -o "$stage/res.zip"
"$tools/aapt2" link -o "$stage/unsigned.apk" --manifest "$stage/AndroidManifest.xml" -I "$jar" -R "$stage/res.zip" -A "$stage/assets"
"$JAVA_HOME/bin/javac" -nowarn --release 17 -classpath "$jar" -d "$stage/classes" $(find crates/app/android/java -name '*.java')
"$tools/d8" --min-api 26 --lib "$jar" --output "$stage" $(find "$stage/classes" -name '*.class')
(cd "$stage" && zip -q -r unsigned.apk lib classes.dex)
"$tools/zipalign" -f -P 16 4 "$stage/unsigned.apk" "$stage/aligned.apk"
key="${ANDROID_KEYSTORE:-$HOME/.android/debug.keystore}"
alias="${ANDROID_KEY_ALIAS:-androiddebugkey}"
export ANDROID_KEYSTORE_PASSWORD="${ANDROID_KEYSTORE_PASSWORD:-android}"
if [ ! -f "$key" ]; then
  mkdir -p "$(dirname "$key")"
  "$JAVA_HOME/bin/keytool" -genkeypair -keystore "$key" -storepass android -keypass android -alias androiddebugkey \
    -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US"
fi
"$tools/apksigner" sign --ks "$key" --ks-pass env:ANDROID_KEYSTORE_PASSWORD --key-pass env:ANDROID_KEYSTORE_PASSWORD \
  --ks-key-alias "$alias" --out "$out/last-harvest.apk" "$stage/aligned.apk"
echo "built $out/last-harvest.apk"
if [ "${1:-}" = --install ]; then
  adb install -r "$out/last-harvest.apk"
fi
