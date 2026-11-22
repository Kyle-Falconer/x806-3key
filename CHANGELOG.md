# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-11-22

### Features

- **cli**: Add x806 cli tool with status, apply, reset, import, export, macro, and udev commands ([3bd01e0](https://github.com/renanalencar/X806/commit/3bd01e0))
- **core**: Add workspace, model, keycodes, and defaults ([f9a9d7c](https://github.com/renanalencar/X806/commit/f9a9d7c))
- **core**: Add dat, ini, and toml codecs with bidirectional roundtrip support ([746613e](https://github.com/renanalencar/X806/commit/746613e))
- **core**: Add deterministic eeprom compiler with safety checks and limits ([06de8cc](https://github.com/renanalencar/X806/commit/06de8cc))
- **core**: Add cross-platform hid transport for USB communication ([be9040f](https://github.com/renanalencar/X806/commit/be9040f))
- **gui**: Add cross-platform egui desktop configurator with keymap and macro recording ([468e877](https://github.com/renanalencar/X806/commit/468e877))

### Bug Fixes

- **core**: Bound codec vec preallocation to prevent memory exhaustion ([bb5c3fb](https://github.com/renanalencar/X806/commit/bb5c3fb))
- **gui**: Harden apply flow and live recording state management ([76bcaa9](https://github.com/renanalencar/X806/commit/76bcaa9))
- **suite**: Address final review findings across codecs and GUI tests ([f8f301c](https://github.com/renanalencar/X806/commit/f8f301c))

### Documentation

- Add x806 protocol spec, design, and plan ([97ccf34](https://github.com/renanalencar/X806/commit/97ccf34))
- Add agents guide, license, and visual captures ([6d67e66](https://github.com/renanalencar/X806/commit/6d67e66))
