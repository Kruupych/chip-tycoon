Chip Tycoon — экономико-технологический симулятор индустрии CPU/GPU

Цель: детерминированная, модульная симуляция индустрии полупроводников с 1990-х до будущего. Ядро на Rust с ECS (Bevy), данные в SQLite/Parquet, UI — Bevy и Tauri+React.

Сборка и проверка

- Требуется Rust stable и компоненты `rustfmt`, `clippy` (см. rust-toolchain.toml).
- Основные команды через `just`:
  - `just build` — сборка воркспейса
  - `just test` — тесты
  - `just lint` — fmt + clippy (c `-D warnings`)
  - `just run-game` — Bevy фронтенд (через `apps/game-frontend`)
  - `just sim` — headless-CLI (через `apps/cli`)

Архитектура

- `crates/sim-core` — доменные модели и инварианты
- `crates/sim-econ` — спрос/прайсинг
- `crates/sim-runtime` — игровой цикл (Bevy ECS)
- `crates/persistence` — SQLite/Parquet снапшоты и телеметрия
- `crates/modkit` — API моддинга (Rhai)
- `crates/sim-ai` — простые политики ИИ
- `crates/data-pipeline` — подготовка и валидация контента
- `apps/cli` — headless утилиты
- `apps/game-frontend` — Bevy-приложение
- `apps/mgmt-ui` — Tauri + React панель управления

Замечания

- Все крейты компилируются с `#![deny(warnings)]`.
- Для моддинга выбран Rhai (pure Rust, проще sandbox), Lua может быть добавлен позже.

## Тюнинг ИИ

- Конфиг по умолчанию: `assets/data/ai_defaults.yaml` (вшивается в бинарь). Там три блока:
  - `weights`: веса утилитарного скоринга (`share`, `margin`, `liquidity`, `portfolio`). Сумма нормализуется автоматически.
  - `planner`: параметры планировщика горизонта (beam width, глубина месяцев, квартальный шаг, величина изменения цены, минимальная маржа и т.п.).
  - `tactics`: тактические пороги и амплитуды (падение доли `share_drop_delta`, шаг цены `price_epsilon_frac`, порог дефицита `shortage_raise_threshold`, минимальная маржа `min_margin_frac`).

- Рекомендации:
  - Для более «агрессивного» завоевания доли — увеличьте `weights.share` и/или `planner.price_pref_beta` (чувствительность доли к цене).
  - Чтобы ИИ осторожнее снижал цену — уменьшите `tactics.price_epsilon_frac` и/или поднимите `tactics.min_margin_frac`.
  - При частых дефицитах — уменьшите `tactics.shortage_raise_threshold` или увеличьте `tactics.shortage_raise_epsilon_frac`.
  - Скорость и качество планировщика: `planner.beam_width` (3–5) и `planner.months` (24–36). Чем больше — тем лучше, но медленнее.

Изменения конфигурации применяются при старте; значения по умолчанию вшиты и используются всегда, если внешний файл недоступен.
- В CI проверяются fmt, clippy и тесты.
 - Артефакты рантайма (saves/, telemetry/, *.db) игнорируются git.

Telemetry

`just sim` сохраняет месячную телеметрию в Parquet-файлы под `./telemetry/`.

Схема (денежные значения в центах, Int64):
- month_index: UInt32
- output_units: UInt64
- sold_units: UInt64
- asp_cents: Int64
- unit_cost_cents: Int64
- margin_cents: Int64
- revenue_cents: Int64
