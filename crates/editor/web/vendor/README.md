# Vendored for the card editor

`WasmMediaEncoder.min.js` (`dist/umd/`) and `ogg.wasm` (`wasm/`) from the npm package
[wasm-media-encoders 0.7.0](https://www.npmjs.com/package/wasm-media-encoders/v/0.7.0), integrity
`sha512-Sp4wUasgxOK/IFfNhpon6LQQgYGwtpxyV4isjGIe1rvhnJL3w2KYr4f+CdqDNtGPDcgCDRY+uBanVfSx6Si0WQ==`. Licenses in `LICENSE`.

The card editor loads them only when a picked sound needs converting to Ogg Vorbis. `cargo deny` doesn't see them:
check the package and libvorbis for advisories by hand when updating.

| File | sha256 |
| --- | --- |
| `WasmMediaEncoder.min.js` | `dd4e17abf5377dfecc726d6ec5e7b72dab01cf3522974278e5347f4e68480fb3` |
| `ogg.wasm` | `cd1f50349e58e650e33e3f0b850a785ed612c4abaa4756e520e50b2583cf07ed` |
