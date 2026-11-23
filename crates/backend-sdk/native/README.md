# native/

The C++ side of the Sony SDK binding lands here in milestone M1.

Intended contents:

- `shim.h` / `shim.cpp` — a C ABI over the SDK. Plain functions taking opaque
  handles and plain structs, with callbacks passed as function pointers plus a
  `void*` user pointer. No C++ types cross this line.
- `CMakeLists.txt` — builds the shim as a static library. Sony's own libraries are
  linked from `SONY_SDK_ROOT`, never copied into this directory.

Rules:

1. **Never commit Sony headers, libraries, samples or documentation.** The build
   reads `SONY_SDK_ROOT` from the environment. Reviewers reject any patch that
   adds vendor files here.
2. Everything in this directory must be C++17 or later, compiled with the same
   C runtime (`/MD` on Windows) as the SDK's DLLs. A mismatch causes silent
   crashes.
3. Callbacks from the SDK arrive on SDK-owned threads. The shim must not block in
   them: push to a queue and return immediately.

`crates/backend-sdk/src/lib.rs` is the only Rust file allowed to call the `cxx`
bridge built on top of this shim.