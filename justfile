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

# Запуск headless-симуляции (пример)
sim:
    cargo run -p cli -- --scenario assets/data/baseline_1990.yaml --years 10

# Запуск Bevy-фронта (игры)
run-game:
    cargo run -p game-frontend

# Запуск Tauri UI
run-ui:
    cd apps/mgmt-ui && pnpm i && pnpm tauri dev

db-migrate:
    sqlx migrate run --database-url sqlite://./saves/main.db

db-repl:
    sqlite3 ./saves/main.db

gen-fixtures:
    cargo run -p data-pipeline -- --generate --out assets/data

snap:
    cargo run -p cli -- --snapshot ./saves/snap.bin

profile:
    cargo criterion --bench sim_bench
