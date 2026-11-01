# shellcheck shell=bash
# Sourced by the web build scripts: stops unless wasm-bindgen matches Cargo.lock, and adds the wasm target.
want=$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/[^0-9.]/, ""); print; exit }' Cargo.lock)
have=$(wasm-bindgen --version 2>/dev/null | awk '{ print $2 }' || true)
if [ "$have" != "$want" ]; then
  echo "wasm-bindgen $want is needed (have: ${have:-none}): cargo install wasm-bindgen-cli --version $want" >&2
  exit 1
fi
rustup target list --installed | grep -qx wasm32-unknown-unknown || rustup target add wasm32-unknown-unknown
