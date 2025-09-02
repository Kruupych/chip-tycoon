# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project adheres to Semantic Versioning.

## [0.1.0] - 2025-09-02
### Added
- Tutorial (24m) with guidance and UI steps.
- Difficulty presets (easy/normal/hard) affecting AI, markets, and events.
- Save/Load with autosave (quarterly), transactional status, and rotation (last 6).
- Campaign export (dry-run) to JSON/Parquet without mutating active world (CLI and IPC/UI).
- i18n RU/EN coverage and tests; UI polish with toasts and chart legend/axes.
- 1990s campaign scenario and markets/tech era assets.
- ModEngine integration via ECS with tech/market events.

### Changed
- Finance cash-flow reconciliation and monthly KPI snapshotting.
- Charts: formatted axes, legend, and compact grid.

### Fixed
- Mod double-apply bug (market effects not applied twice at same start).

### CI
- GitHub Actions workflow with lint/test/build and commit message guard.

