# How the app finds Sony's SDK

Sony's licence decides whether you may ship their libraries with your app or
whether the user installs them. We do not know yet which applies. So the app
**finds the SDK at runtime** instead of linking it at build time. One binary then
covers every answer.

| If Sony's agreement says | Deployment | Same binary? |
|---|---|---|
| You may distribute the runtime libraries | Installer places them in `sdk/` beside the exe | Yes |
| You may not redistribute | Unsigned installer asks the user to point at their own SDK copy | Yes |
| No answer yet | Development uses `TETHER_SDK_DIR`; ship when the clause is read | Yes |

A second benefit: a user whose camera firmware is newer than our build can drop in
a newer SDK without waiting for a release, as long as our shim still works with it.

## Directory layout

```
tether-app.exe
sdk/
  manifest.json                 lists what must be present
  tether_sdk_shim.dll           our shim, built from crates/backend-sdk/native
  <Sony's SDK libraries>        names recorded in M1, from the real SDK
```

## `manifest.json`

```json
{
  "schema_version": 1,
  "files": [
    "tether_sdk_shim.dll",
    "<sony library>.dll"
  ],
  "notes": "Sony Camera Remote SDK, obtained by the user"
}
```

Rules:

- Entries must be plain filenames. Separators, drive letters and `..` are
  rejected. A manifest comes from outside the program and is treated as hostile.
- `schema_version` must match the constant in `crates/backend-sdk/src/loader.rs`.
- Every listed file must exist, or the app refuses to load and names the ones
  that are missing.

Sony's filenames are not hard-coded anywhere in the source. They are recorded in
the manifest in milestone M1, once the real SDK is in hand.

## Search order

1. `TETHER_SDK_DIR`, if set.
2. `sdk/` beside the executable. This is what an installer creates.
3. `sdk/` in the current working directory, for development.

The first path that **exists** wins. If it exists but fails validation, that
error is reported as is, because a broken `sdk/` next to the exe is the common
real failure and a silent fallback would hide it.

## When nothing is found

The app still starts. It shows the red banner, and the log line names every path
it tried. `SdkBackend::connect` returns the same list as an error, so a bug report
carries the diagnosis.

## Packaging

`cargo xtask release` builds and packages a zip into `dist/`, and prints its SHA-256.

| Mode | Command | What ships | Can tether |
|---|---|---|---|
| Stub | `cargo xtask release --allow-stub` | exe, docs | No. The tool refuses without the flag, and names the artifact `...-stub` |
| SDK, user installs | `cargo xtask release` with `SONY_SDK_ROOT` set | exe, docs | Yes, once the user points `TETHER_SDK_DIR` at their SDK |
| SDK, bundled | `cargo xtask release --bundle-sdk <dir>` | exe, docs, `sdk/` | Yes, out of the box. Only if the licence allows redistribution |

Every package contains `BUILD-INFO.txt` stating the version, the commit, and whether the build can tether. Read it before publishing anything.

## What lands in M1

- `dlopen` / `LoadLibrary` of `SHIM_FILE` from `SdkLocation::file()`.
- A version handshake between the shim and Rust, so a stale shim is rejected with
  a sentence instead of a crash.
- Signature of the manifest entries: record file size and SHA-256, so a truncated
  or wrong-architecture copy is caught before loading.