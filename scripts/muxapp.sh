#!/usr/bin/env bash
# Builds target/muos/LastHarvest.muxapp for muOS on the 64-bit H700 Anbernics: the game linked against the device's
# own libSDL2 (copied once from a device by ssh), its pack, the launcher and the menu icon. Install it through the
# device's Archive Manager. Run scripts/sysroot.sh once first. MUOS_HOST names the device to copy libSDL2 from
# (default muos), or MUOS_SDL a libSDL2 to link against instead; --install also unpacks the app over ssh onto that
# device's first card. --release leaves game_data.pak out and names LAST_HARVEST_VERSION's release in version.txt, for
# the launcher to fetch that release's shared pack.
set -euo pipefail
cd "$(dirname "$0")/.."
host="${MUOS_HOST:-muos}"
out=target/muos
sdl="$out/sdl/libSDL2.so"
mkdir -p "$out/sdl"
if [ -n "${MUOS_SDL:-}" ]; then
  cp -L "$MUOS_SDL" "$sdl"
elif [ ! -f "$sdl" ]; then
  scp -q "$host:/usr/lib/libSDL2-2.0.so.0" "$sdl"
fi
release=""
[ "${1:-}" = --release ] && release=1
cargo run -q -p last-harvest-pack
sr="$PWD/.sysroot"
# The device's libSDL2 needs its Mali driver, which only the device has; the binary loads both there.
PKG_CONFIG="$PWD/scripts/pkg-config" PKG_CONFIG_ALLOW_CROSS=1 PKG_CONFIG_SYSROOT_DIR="$sr" PKG_CONFIG_LIBDIR="$sr/usr/lib/pkgconfig" \
  RUSTFLAGS="-L $sr/usr/lib -L $PWD/$out/sdl -C link-arg=-Wl,--allow-shlib-undefined" CARGO_PROFILE_RELEASE_STRIP=symbols CARGO_PROFILE_RELEASE_DEBUG=false \
  cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 -p last-harvest --bin last-harvest-muos --no-default-features --features muos

stage="$out/stage"
app="$stage/LastHarvest"
rm -rf "$stage"
mkdir -p "$app/data" "$app/glyph"
cp crates/app/muos/mux_launch.sh "$app/"
cp crates/app/muos/glyph/lastharvest.png "$app/glyph/"
cp target/aarch64-unknown-linux-gnu/release/last-harvest-muos "$app/last-harvest"
if [ -n "$release" ]; then
  echo "${LAST_HARVEST_VERSION:?a release needs LAST_HARVEST_VERSION}" >"$app/version.txt"
else
  cp target/game_data.pak "$app/data/"
fi
chmod +x "$app/mux_launch.sh" "$app/last-harvest"
rm -f "$out/LastHarvest.muxapp"
(cd "$stage" && find LastHarvest | LC_ALL=C sort | zip -q -X ../LastHarvest.muxapp -@)
echo "built $out/LastHarvest.muxapp"
if [ "${1:-}" = --install ]; then
  dir=/mnt/mmc/MUOS/application
  scp -q "$out/LastHarvest.muxapp" "$host:/tmp/LastHarvest.muxapp"
  ssh "$host" "mkdir -p $dir && cd $dir && unzip -qo /tmp/LastHarvest.muxapp && rm /tmp/LastHarvest.muxapp"
  echo "installed in $host:$dir/LastHarvest"
fi
