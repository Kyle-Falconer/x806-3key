# Repository Guidelines

## Project Structure & Module Organization

This Rust 2024 workspace has three crates under `crates/`: `x806-core` contains device protocol, HID transport, codecs, keycodes, and EEPROM compilation; `x806-cli` provides the `x806` command-line app; and `x806-gui` contains the `egui` desktop app. Integration tests live in each crate's `tests/` directory. Protocol documentation is in `docs/protocol-spec.md`; Linux device permissions are defined in `udev/99-x806.rules`.

## Build, Test, and Development Commands

Run commands from the repository root:

- `cargo build --workspace` builds all crates; add `--release` for optimized binaries.
- `cargo run -p x806-cli -- status` runs a read-only device status probe.
- `cargo run -p x806-gui` launches the desktop configurator.
- `cargo test --workspace` runs unit and integration tests, including codec, EEPROM, HID mock, CLI, and GUI tests.
- `cargo clippy --workspace --all-targets -- -D warnings` checks all targets with warnings treated as errors.

## Coding Style & Naming Conventions

Follow standard Rust formatting: four-space indentation, `snake_case` for modules, functions, and variables, and `PascalCase` for types and traits. Keep crate responsibilities clear and share device or format logic through `x806-core`. Run `cargo fmt --all` before submitting changes.

## Testing Guidelines

Add or update tests alongside the affected crate, using `tests/*.rs` for integration coverage. Name test files after the behavior or subsystem (for example, `eeprom_compiler.rs` or `cli.rs`). Prefer offline tests and mocked HID interactions so test runs do not require a physical keypad. Run `cargo test --workspace` and the Clippy command above after changes.

## Commit & Pull Request Guidelines

Recent commits use Conventional Commit-style subjects, typically `type(scope): imperative summary`, such as `feat(core): add deterministic eeprom compiler` or `fix(gui): harden apply flow`. Keep commits focused. Pull requests should explain user-visible and protocol changes, link related issues when available, list validation commands and results, and include screenshots for GUI changes.

All commits must follow Conventional Commits (`feat:`, `fix:`, `docs:`, `perf:`, `refactor:`, `test:`, `chore:`) to ensure automated semantic version bumping, changelog updates, and release workflows (`release-plz`, `git-cliff`) operate correctly.

## Versioning & Releases

- Workspace releases use Semantic Versioning (`MAJOR.MINOR.PATCH`).
- Automated releases run through `.github/workflows/release.yml` using `release-plz`. When commits land on `main`, `release-plz` analyzes commit types and creates a Release PR with updated versions and `CHANGELOG.md`.
- `release-plz.toml` and `cliff.toml` configure automated versioning and changelog parsing.
- For local release verification or manual version bumping, use `./scripts/release.sh [--dry-run]`.

## Device Safety

EEPROM writes affect physical hardware. Preserve `x806-core` validation and protected-byte safeguards; cover changes to write planning with tests. Keep hardware access opt-in in CLI and GUI flows, and use `--dry-run` when reviewing a proposed configuration.
