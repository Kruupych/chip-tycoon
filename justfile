# justfile — команды для разработки
set shell := ["bash", "-cu"]

# Сборка всего воркспейса
build:
    cargo build --workspace

# Прогон всех тестов
test:
    cargo test --workspace --all-features

# Линтеры и автоформатирование
lint:
    cargo fmt --all
    cargo clippy --workspace --all-features -- -D warnings

# Бенчмарки (criterion)
bench:
    cargo criterion

ai-bench:
    cargo criterion --bench ai_bench

# Запуск headless-симуляции с произвольными аргументами
sim *ARGS:
    cargo run -p cli -- {{ARGS}}

# Упрощённый запуск кампании 1990s (по умолчанию)
sim-campaign WHICH="1990s":
    cargo run -p cli -- --campaign {{WHICH}}

export-campaign path="telemetry/campaign_1990s.json":
    cargo run -p cli -- --campaign 1990s --export-campaign {{path}}

# Защитник: не даёт запускать сборки с грязным деревом
guard-clean:
    git diff --quiet && git diff --cached --quiet || (echo "❌ Working tree not clean"; exit 1)

# Композитная CI-цель
ci: guard-clean lint test build

# Frontend tests (optional)
test-ui:
    cd apps/mgmt-ui/web && pnpm i && pnpm vitest run --environment jsdom

# Запуск Bevy-фронта (игры)
run-game:
    cargo run -p game-frontend

# Запуск Tauri UI
run-ui:
    cd apps/mgmt-ui && pnpm i && pnpm tauri dev

db-migrate:
    cargo run -p persistence --bin migrate

db-repl:
    sqlite3 ./saves/main.db

gen-fixtures:
    cargo run -p data-pipeline -- --generate --out assets/data

snap:
    cargo run -p cli -- --snapshot ./saves/snap.bin

profile:
    cargo criterion --bench sim_bench

# Release builds
release-cli:
    cargo build -p cli --release

release-ui:
    # Build Tauri UI only if a Tauri config is present
    if [ -f apps/mgmt-ui/src-tauri/tauri.conf.json ] || [ -f apps/mgmt-ui/tauri.conf.json ] || [ -f apps/mgmt-ui/src-tauri/Tauri.toml ]; then \
      if [ -f apps/mgmt-ui/package.json ]; then \
        cd apps/mgmt-ui && pnpm i && pnpm tauri build; \
      else \
        echo "UI package.json missing in apps/mgmt-ui; skipping UI build"; \
      fi; \
    else \
      echo "No Tauri config found; skipping UI build"; \
    fi

release-all: release-cli release-ui
    VER=$(sed -n 's/^version = \"\(.*\)\"/\1/p' Cargo.toml | head -n1); ARCH=$(uname -s 2>/dev/null | tr '[:upper:]' '[:lower:]' | awk '{ if($0 ~ /mingw|msys|cygwin|windows/) print "windows-x64"; else if($0 ~ /darwin/) print "macos-x64"; else print "linux-x64" }'); OUT="dist/v$VER/$ARCH"; mkdir -p "$OUT"; if [ -f target/release/cli ]; then cp -v target/release/cli "$OUT/"; fi; rsync -a --delete --exclude 'saves' --exclude 'telemetry' assets "$OUT/" || true; cp -v README_quickstart.md "$OUT/" || true; if [ -d apps/mgmt-ui/src-tauri/target/release/bundle ]; then mkdir -p "$OUT/mgmt-ui" && cp -rv apps/mgmt-ui/src-tauri/target/release/bundle "$OUT/mgmt-ui/"; elif [ -d apps/mgmt-ui/web/src-tauri/target/release/bundle ]; then mkdir -p "$OUT/mgmt-ui" && cp -rv apps/mgmt-ui/web/src-tauri/target/release/bundle "$OUT/mgmt-ui/"; fi

# Windows-only UI build (PowerShell)
release-ui-win:
    if [ "$(uname -s | tr '[:upper:]' '[:lower:]')" != "windows_nt" ] && [ -z "$WIN" ]; then \
      echo "Windows UI build is skipped on non-Windows; run 'just release-ui-win' in Windows PowerShell"; \
      exit 0; \
    fi
    pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/windows/build-ui.ps1

# Package Windows artifacts to zip
package-win:
    VER=$(sed -n 's/^version = \"\(.*\)\"/\1/p' Cargo.toml | head -n1); SRC="dist/v$VER/windows-x64"; ZIP="chip-tycoon-$VER-windows-x64.zip"; \
    if command -v pwsh >/dev/null 2>&1; then pwsh -NoProfile -Command "Compress-Archive -Force -Path '$$PWD/$SRC/*' -DestinationPath '$$PWD/$ZIP'"; else zip -r "$ZIP" "$SRC"; fi

# Check Tauri environment presence
check-tauri:
    echo "Checking Tauri setup..."; \
    if [ -f apps/mgmt-ui/src-tauri/tauri.conf.json ] || [ -f apps/mgmt-ui/tauri.conf.json ] || [ -f apps/mgmt-ui/src-tauri/Tauri.toml ]; then echo "- Tauri config: present"; else echo "- Tauri config: missing"; fi; \
    if command -v node >/dev/null 2>&1; then echo "- Node: present"; else echo "- Node: missing"; fi; \
    if command -v pnpm >/dev/null 2>&1; then echo "- pnpm: present"; else echo "- pnpm: missing"; fi; \
    if command -v tauri >/dev/null 2>&1; then echo "- tauri CLI: present (global)"; else echo "- tauri CLI: not found (will use dev-dep)"; fi
