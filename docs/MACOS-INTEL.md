# Building & running on Intel Macs (x86_64-apple-darwin)

`ort` rc.12 no longer ships a prebuilt ONNX Runtime for Intel Macs (pyke dropped
`x86_64-apple-darwin`), so a plain build fails with:

```
error: ort-sys: ort does not provide prebuilt binaries for the target
`x86_64-apple-darwin` with feature set (no features).
```

The repo handles this: on `x86_64-apple-darwin` only, `vieneu-core` enables the
`ort` **`load-dynamic`** feature (see `crates/vieneu-core/Cargo.toml`). The build
then links nothing and loads ONNX Runtime **at runtime** instead — Apple Silicon,
Windows and Linux are unaffected (they keep the downloaded prebuilt).

## 1. Build — works as-is

```bash
make build          # or: make sidecars
```

No special environment is needed at build time.

## 2. Provide ONNX Runtime at runtime

Install ONNX Runtime and point the TTS server at its dynamic library:

```bash
brew install onnxruntime
export ORT_DYLIB_PATH="$(brew --prefix onnxruntime)/lib/libonnxruntime.dylib"
```

`vieneu-server` reads `ORT_DYLIB_PATH` on startup. Export it in the same shell you
launch from so the value is inherited:

```bash
# running the servers directly
export ORT_DYLIB_PATH="$(brew --prefix onnxruntime)/lib/libonnxruntime.dylib"
make server          # vieneu-server (TTS) on :8080

# or the desktop app (it spawns vieneu-server, which inherits the env)
export ORT_DYLIB_PATH="$(brew --prefix onnxruntime)/lib/libonnxruntime.dylib"
make ui-dev
```

Tip: add the `export ORT_DYLIB_PATH=…` line to your `~/.zshrc` so it is always set.

If you hit an ONNX Runtime version-compatibility error with the Homebrew build,
download a matching release from
<https://github.com/microsoft/onnxruntime/releases> (the `osx-x86_64` archive) and
point `ORT_DYLIB_PATH` at its `lib/libonnxruntime.dylib` instead.

Everything else (the studio server, the Tauri shell, ffmpeg, the dubbing sidecar)
builds and runs natively on Intel Macs — see also `docs/WINDOWS.md` for the
shared notes.

## 3. CI builds a self-contained Intel app

`.github/workflows/build.yml` has a `macOS (Intel)` matrix entry (`macos-13`,
`x86_64-apple-darwin`). For that target it downloads ONNX Runtime **1.24.2**
(matching `ort-sys` rc.12; Homebrew fallback) into `ui/src-tauri/runtime/`, which
is bundled via `tauri.conf.json` `resources`. At launch the Tauri shell
(`lib.rs`) sets `ORT_DYLIB_PATH` to that bundled `libonnxruntime.dylib` for the
spawned sidecars — so the **installer runs without a system ONNX Runtime**. On
Apple Silicon / Windows the dir stays empty and `ORT_DYLIB_PATH` is left unset
(ort is linked statically there).
