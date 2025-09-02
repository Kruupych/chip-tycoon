# Release Checklist v0.1.0

- [ ] Bump version in Cargo.toml (workspace) to v0.1.0 (done).
- [ ] Ensure CHANGELOG and release-notes updated (done).
- [ ] Build artifacts: `just release-all` (done).
- [ ] Verify dist contents under `dist/v0.1.0/<platform>/`:
  - [ ] CLI binary present; `./cli --version` prints version/git/date.
  - [ ] `assets/` present.
  - [ ] UI bundles under `mgmt-ui/bundle/`.
  - [ ] `README_quickstart.md` present.
- [ ] IPC build info: run mgmt-ui and check `sim_build_info()` returns version/git_sha/build_date.
- [ ] Run `just sim-campaign` – KPI line and Success outcome.
- [ ] Tag release: `git tag -a v0.1.0 -m "chip-tycoon v0.1.0"`.
- [ ] Publish GitHub Release with `docs/release-notes-0.1.0.md`.
