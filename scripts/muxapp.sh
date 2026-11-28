#!/usr/bin/env bash
# Builds target/muos/LastHarvest.muxapp for muOS on the 64-bit H700 Anbernics: the game linked against the device's
# own libSDL2 (copied once from a device by ssh), its pack, the launcher and the menu icon. Install it through the
# device's Archive Manager. Run scripts/sysroot.sh once first. MUOS_HOST names the device to copy libSDL2 from
# (default muos), or MUOS_SDL a libSDL2 to link against instead. --release leaves game_data.pak out and names
# LAST_HARVEST_VERSION's release in version.txt, for the launcher to fetch that release's shared pack.
# --install unpacks it over ssh where LastHarvest already is; --install-release TAG installs that release and its pack.
# Options and variables: docs/platforms.md.
set -euo pipefail
cd "$(dirname "$0")/.."
host="${MUOS_HOST:-muos}"
root="${MUOS_ROOT:-}"
out=target/muos
. scripts/lib/target.sh
release="" install="" tag="" dry=""
while [ $# -gt 0 ]; do
  case "$1" in
    --release) release=1 ;;
    --install) install=1 ;;
    --install-release) install=1 tag="${2:?--install-release needs a tag}"; shift ;;
    --dry-run) dry=1 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

build() {
  local sdl="$out/sdl/libSDL2.so"
  mkdir -p "$out/sdl"
  if [ -n "${MUOS_SDL:-}" ]; then
    cp -L "$MUOS_SDL" "$sdl"
  elif [ ! -f "$sdl" ]; then
    scp -q "$host:/usr/lib/libSDL2-2.0.so.0" "$sdl"
  fi
  cargo run -q -p last-harvest-pack
  rustup target list --installed | grep -qx aarch64-unknown-linux-gnu || rustup target add aarch64-unknown-linux-gnu
  local sr="$PWD/.sysroot"
  # The device's libSDL2 needs its Mali driver, which only the device has; the binary loads both there.
  PKG_CONFIG="$PWD/scripts/pkg-config" PKG_CONFIG_ALLOW_CROSS=1 PKG_CONFIG_SYSROOT_DIR="$sr" PKG_CONFIG_LIBDIR="$sr/usr/lib/pkgconfig" \
    RUSTFLAGS="-L $sr/usr/lib -L $PWD/$out/sdl -C link-arg=-Wl,--allow-shlib-undefined" CARGO_PROFILE_RELEASE_STRIP=symbols CARGO_PROFILE_RELEASE_DEBUG=false \
    cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 -p last-harvest --bin last-harvest-muos --no-default-features --features muos

  local stage="$out/stage"
  local app="$stage/LastHarvest"
  rm -rf "$stage"
  mkdir -p "$app/data" "$app/glyph"
  cp crates/app/muos/mux_launch.sh "$app/"
  cp assets/private/icons/muos-glyph.png "$app/glyph/lastharvest.png"
  cp "$bin/aarch64-unknown-linux-gnu/release/last-harvest-muos" "$app/last-harvest"
  if [ -n "$release" ]; then
    echo "${LAST_HARVEST_VERSION:?a release needs LAST_HARVEST_VERSION}" >"$app/version.txt"
  else
    cp target/game_data.pak "$app/data/"
  fi
  chmod +x "$app/mux_launch.sh" "$app/last-harvest"
  rm -f "$out/LastHarvest.muxapp"
  (cd "$stage" && find LastHarvest | LC_ALL=C sort | zip -q -X ../LastHarvest.muxapp -@)
  echo "built $out/LastHarvest.muxapp"
}

# With MUOS_ROOT set, "the device" is that local directory.
on_device() {
  if [ -n "$root" ]; then sh -s; else ssh "$host" sh -s; fi
}
put() {
  if [ -n "$dry" ]; then
    echo "would copy $1 to ${root:-$host:}$2"
  elif [ -n "$root" ]; then
    mkdir -p "$root$(dirname "$2")" && cp "$1" "$root$2"
  else
    scp -q "$1" "$host:$2"
  fi
}
run_on_device() {
  if [ -n "$dry" ]; then
    echo "would run on ${root:-$host}:"
    sed 's/^/  /'
  else
    on_device
  fi
}

find_dir() {
  on_device <<EOF
r='$root'
both=""
for d in /mnt/sdcard/MUOS/application /mnt/mmc/MUOS/application; do
  [ -d "\$r\$d/LastHarvest" ] && both="\$both \$d"
done
set -- \$both
[ \$# -gt 1 ] && echo "LastHarvest is on both cards; installing over the one on card 2 (\$1)" >&2
if [ \$# -gt 0 ]; then echo "\$1"
elif grep -q ' /mnt/sdcard ' "\$r/proc/mounts" 2>/dev/null; then echo /mnt/sdcard/MUOS/application
else echo /mnt/mmc/MUOS/application
fi
EOF
}

if [ -n "$tag" ]; then
  pkg="$out/release/$tag"
  rm -rf "$pkg"
  mkdir -p "$pkg"
  gh release download "$tag" -R KyleJamesWalker/last-harvest -D "$pkg" -p "last-harvest-$tag-muos.muxapp" -p game_data.pak
  muxapp="$pkg/last-harvest-$tag-muos.muxapp"
else
  build
  muxapp="$out/LastHarvest.muxapp"
fi
[ -n "$install" ] || exit 0

dir="${MUOS_DIR:-$(find_dir)}"
put "$muxapp" /tmp/LastHarvest.muxapp
[ -n "$tag" ] && put "$pkg/game_data.pak" /tmp/game_data.pak
run_on_device <<EOF
set -e
r='$root'
mkdir -p "\$r$dir"
cd "\$r$dir"
rm -f LastHarvest/version.txt LastHarvest/data/pak-version
unzip -qo "\$r/tmp/LastHarvest.muxapp"
rm "\$r/tmp/LastHarvest.muxapp"
if [ -n '$tag' ]; then
  mkdir -p LastHarvest/data
  mv "\$r/tmp/game_data.pak" LastHarvest/data/game_data.pak
  echo '$tag' >LastHarvest/data/pak-version
fi
EOF
[ -n "$dry" ] || echo "installed in ${root:-$host:}$dir/LastHarvest"
