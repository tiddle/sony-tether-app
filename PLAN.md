# Plan: Sony A7 tether app

**Status:** planning complete, skeleton committed, blocked on Sony SDK approval.
**Target body:** A7 IV (`ILCE-7M4`), firmware 6.02.
**First platform:** Windows 10/11 x64.

## 1. What we are building

A desktop app that tethers to a Sony A7 IV over USB. It shows live view, fires the shutter, and saves every capture into a folder on the computer. A photographer uses it to shoot with the camera on a stand and see the framing on a large screen.

The first version is deliberately small. Exposure control, focus point, hot folders and wireless come later.

## 2. Decisions made

| Decision | Choice | Reason |
|---|---|---|
| Camera control API | Sony Camera Remote SDK | Only supported route to live view and control on this body |
| Language | Rust | Memory safety, good Windows tooling, single binary |
| UI | `egui` / `eframe` | Pure Rust, no WebView dependency, live-view texture upload is trivial |
| Binding style | `cxx` crate over a thin C ABI shim | The SDK is callback-based C++; the shim keeps one unsafe boundary |
| Concurrency | Threads and `std::sync::mpsc`, no async runtime | The SDK is synchronous and callback-driven. Tokio would add cost without benefit |
| Licence | MIT for our code | Cheap now, reversible while we are the sole author |
| SDK in repo | Never | Sony's licence does not permit redistribution of headers or libraries |
| First body | A7 IV only | Broad A7 coverage would double the scope for no MVP value |

## 3. Scope of the first usable version

**In:** USB connection to the A7 IV; live view; half-press and full release; automatic download of each capture into a chosen folder with a numbered filename; thumbnail strip of recent shots; connection status; plain-text log recording body model, firmware and SDK version.

**Out for now:** exposure control, autofocus point, hot folder into Lightroom or Capture One, in-app review and rating, video, wireless, every body except the A7 IV.

Exposure control is deferred on purpose. It is the feature that multiplies per-body testing more than any other, and it is the easiest to add once the transport path is proven.

### Acceptance criteria

1. Live view runs at 15 fps or better at roughly 1024 px wide, with no stutter and no growth in memory.
2. Shutter to file on disk is under 3 seconds for JPEG and under 8 seconds for a 33 MP RAW.
3. The downloaded RAW opens in Lightroom and its byte size matches the file on the card.
4. Unplug and replug recovers without restarting the app, within 5 seconds.
5. Runs on Windows 10 and 11, x64, with no administrator rights.
6. A second SDK-supported body passes the same tests, or that body is documented as unsupported.

## 4. Why the official SDK

Sony publishes no open protocol for full tethered shooting. The alternatives:

| Route | Live view | Shutter | Download | Verdict |
|---|---|---|---|---|
| Sony Camera Remote SDK | Yes | Yes | Yes | Chosen. Supports the A7 IV |
| Legacy Camera Remote API (JSON-RPC) | Yes, low rate | Yes | JPEG only, Wi-Fi only | Not available on this body over USB |
| libgphoto2 / raw PTP | Weak or absent on Sony | Sometimes | Yes | Fallback experiment only |

The SDK supports a narrow window of bodies, roughly 2020 and later. Older A7 bodies need a separate adapter if they are ever in scope.

## 5. Architecture

```
 tether-app (egui)                      backend-sdk (Rust)
   status, live view, shutter  ────────►  CameraBackend impl
   capture list, log                      │
        ▲                                 ▼
        │                        native/ C ABI shim (cxx)
   camera-core                     Sony Camera Remote SDK (.dll)
   traits, events, session,                │
   capability matrix                       ▼
                                     USB → A7 IV
```

Crates:

- `crates/camera-core` — the `CameraBackend` trait, event and type definitions, the session folder and filename rules, and the capability matrix. No Sony code.
- `crates/backend-sdk` — the only crate that touches Sony. Owns runtime discovery of the SDK (`loader`), the C++ shim under `native/`, and the translation of SDK callbacks into `BackendEvent`s.
- `apps/tether-app` — the egui window and nothing else.

Rules that keep the layering honest:

1. `camera-core` never imports `backend-sdk`. The app picks a backend at startup.
2. Everything crossing the trait boundary is plain Rust data. All of it is `serde`-serialisable, so moving the SDK into a supervised child process later (if unplug crashes prove it necessary) is a small change.
3. Only `backend-sdk` may contain `unsafe`.

## 6. The capability matrix is data, not code

`crates/camera-core/data/cameras.json` lists what is known per body, per feature, with four states: `verified`, `unverified`, `unsupported`, `out_of_scope`.

Rules:

- Only `verified` may enable a control in the UI. Unknown bodies and unknown features default to `unverified`, never to supported.
- A row changes to `verified` only after the acceptance tests pass on that firmware, and the firmware version goes in the row.
- The app logs the connected model and firmware at connect time. That log line is the input to every future matrix row.

## 7. SDK setup, loading and licence rules

**Build time**

1. The repository never contains Sony headers, libraries, samples or documentation.
2. `SONY_SDK_ROOT` is where our C++ shim compiles against Sony's headers. `crates/backend-sdk/build.rs` warns and compiles a stub backend when it is unset. The stub reports "SDK not configured" and never pretends to work.
3. The stub exists for developer convenience only. **Release builds must set `SONY_SDK_ROOT`.** Shipping a stub is a defect.

**Runtime**

4. The app loads the shim and Sony's libraries from a directory described in `docs/sdk-layout.md`. Search order: `TETHER_SDK_DIR`, then `sdk/` beside the exe, then `sdk/` in the working directory. Nothing from the vendor is statically linked into the exe.
5. A `manifest.json` in that directory lists the files that must be present. Sony's filenames are recorded there in M1, never hard-coded in the source.

**Why runtime instead of static linkage**

6. Sony's agreement decides whether their libraries may be shipped with the app or must be installed by the user. Runtime loading means one binary covers every answer: bundles fine, user installs it, or undecided. It also lets a user drop in a newer SDK for newer camera firmware.
7. **The distribution question is open on purpose.** Do not assume bundling is permitted. Read the agreement, and get Sony's developer support to confirm in writing.

**Everything else**

8. On first run the app shows Sony's EULA and records acceptance. It does not proceed until the user accepts.
9. Do not use "Sony" in the product name. Use "works with" wording in documentation and the binary's description. See `README.md`.
10. The app is MIT. Switching to a proprietary licence is cheap only while we are the sole author: today it is a text edit, after an outside pull request it is a negotiation. Revisit before accepting contributions from others.

Because the MIT licence cannot cover Sony's SDK, the combination is not free software in the strict sense. That is a constraint of the platform, not of this project.

## 8. Continuous integration

`.github/workflows/ci.yml` runs on `windows-latest` on every push and pull request. It checks formatting, lints the two camera-independent crates, runs their tests, and builds the app with `SONY_SDK_ROOT` unset.

That last detail is the point: until now the Windows/MSVC build path had never been exercised. CI closes that gap for free.

CI **cannot** build or test the SDK path. Sony's licence keeps the SDK out of a public repository, a hosted runner cannot download it, and smuggling it through secrets would be redistribution. So:

- Release builds come from a self-hosted runner on the developer's Windows PC, or from that PC by hand.
- A green run proves compilation and unit tests, not tethering. Tethering is proved on a desk, with the camera plugged in.
- Automated tethering tests need a self-hosted runner with the body attached. Worth adding from M2 onward; not worth it before M1 works.

## 9. Milestones

| | Work | Exit test |
|---|---|---|
| **M0** | Finish Sony developer registration. On approval, confirm the download has Windows x64 libraries, and compare firmware 6.02 against the SDK's tested-firmware list | Sony's own sample builds and runs |
| **M1** | `cxx` shim and `SdkBackend::connect`. Dump properties, save one live-view frame, capture one RAW | The RAW opens, and the matrix row for `ILCE-7M4` is filled in with evidence |
| **M2** | Real UI: live view texture, half press and release, download worker thread, numbered filenames, thumbnails | Acceptance criteria 1–3 |
| **M3** | Unplug and replug, camera sleep and wake, USB error recovery, settings persistence, log file | Acceptance criteria 4–5 |
| **M4** | Packaging: portable zip and installer, bundled C runtime, EULA gate, first-run note about PC Remote mode | A clean Windows machine runs it with no setup |
| **M5** | Wireless, and any second body. Only after M4 ships | Not committed |

M0 is the only blocking step and the only one that needs an action outside this repository.

## 10. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| SDK approval is slow or refused | Nothing can be built | M1 is small, so it starts the day approval lands. Ask early about the redistribution terms |
| Firmware 6.02 is newer than the SDK's tested versions | Unpredictable SDK behaviour | Check the SDK release notes first and update the SDK before debugging |
| The SDK crashes the process on abrupt unplug | Lost session, angry user | Reconnect logic in M3. Keep the process split option open via serialisable boundary types |
| C runtime mismatch between our build and the SDK DLLs | Silent crashes on Windows | Build with the same `/MD` runtime; test on a clean machine in M4 |
| Live-view decode is CPU heavy | Stutter, dropped frames | Decode off the UI thread, drop late frames instead of queueing them |
| USB power while in PC Remote mode | Battery drains during a shoot | Test a powered hub or PD cable in M1, document what works |

## 11. Deliberately deferred

- Exposure control, focus point, bracketing, timelapse.
- Hot folder and Lightroom or Capture One integration.
- Wireless in any form.
- Any body except the A7 IV.
- Persistent settings beyond the download folder.
- i18n, theming, plugin API.

Adding any of these before M4 ships is a scope change, not a detail.