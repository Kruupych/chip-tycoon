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
