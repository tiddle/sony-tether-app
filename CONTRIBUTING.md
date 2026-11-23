# Contributing

## Licence of your contribution

This project is MIT licensed. By opening a pull request you confirm two things:

1. You wrote the code, or you have the right to submit it.
2. You grant the project owner a perpetual, worldwide, royalty-free right to use, modify, and relicense your contribution, including under a different licence.

That second point is what keeps the option of a paid build open later. It is a short summary, not a formal Contributor Licence Agreement. If this project grows beyond a handful of contributors, replace this section with a real CLA.

## Hard rule: no vendor files

Never commit Sony headers, libraries, samples or documentation. The build reads `SONY_SDK_ROOT` from the environment. A pull request that adds vendor binaries or headers is rejected without review, because it would put the project in breach of Sony's licence.

## Before you open a pull request

```sh
cargo test
cargo check --workspace --all-targets
```

Both should be clean. If your change touches a camera path, say in the pull request which body and firmware you tested on. That is the only way a capability matrix row moves from `unverified` to `verified`.

## Code rules

- Nothing outside `crates/backend-sdk` may contain `unsafe`.
- Everything that crosses the `CameraBackend` trait must be plain, serialisable data.
- Unknown camera behaviour is `unverified`, never assumed to work.
- A disabled control in the UI must say why it is disabled.

## Reporting a tethering bug

Include the app's log line for the connected body: model, firmware, and SDK version. Without those three, a tethering bug is not reproducible.