Contributing Guidelines

Testing policy

- UI tests (Vitest, jsdom) are mandatory. Do not disable or skip tests in PRs. Using it.skip/describe.skip is not allowed.
- just test must run both Rust tests and UI tests. Keep just test-ui for local runs, but it is invoked by just test.
- If a test is flaky, prefer stable selectors (data-testid) over skipping. You may leave at most one it.todo with a clear TODO, but the suite must remain green.
- IPC names and payloads must strictly match Tauri commands. Update both frontend and backend as needed.

Local commands

- Run: `just lint && just test && just build`
- UI interactive run: `just run-ui`

