# Contributing

Keep changes focused and describe the problem they solve. For larger changes, open an issue first to discuss the intended behavior.

## Development

Use Windows 10/11 x64, stable Rust with the MSVC toolchain, and Visual Studio C++ Build Tools with a Windows SDK. See the [README](README.md#build-from-source) for build and packaging instructions.

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Formatting follows `rustfmt.toml`; CI rejects unformatted code and Clippy warnings.

Match the surrounding code style. Keep each module about one thing: the main window, for example, has one file per tab under `crates/bsm-app/src/main_window/pages`. Add or update tests for behavior changes where practical. Keep `Cargo.lock` committed so application builds use the reviewed dependency versions.

Unit tests use fixtures and local test services; they do not require a game installation. Game-server integration checks require your own installed game and server files. Use a separate `BSM_DATA_DIR` and follow the [menu self-test guide](docs/menu-selftest/README.md).

## Bug reports

Include the application version, Windows version, steps to reproduce, expected behavior, and actual behavior. For game issues, include the map, Workshop IDs, mods, and whether the issue also occurs without mods. Include relevant log excerpts or screenshots, removing account names, passwords, connection addresses, and personal paths before posting.

## Pull requests

- Explain the user-visible change and any compatibility implications.
- List checks performed and distinguish automated tests from in-game validation.
- Update relevant documentation when controls, setup, or behavior change.
- Exclude compiled binaries, runtime data, game files, downloaded clients, and credentials.

The main UI, classic UI, and CLI share `bsm-core`; consider all three entry points when changing common behavior.
