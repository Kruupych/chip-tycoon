# mgmt-ui (Tauri + React)

Этот каталог зарезервирован для будущего UI на Tauri+React. 
Сборка/разработка потребует Node.js и pnpm. Скаффолдинг будет добавлен на соответствующей фазе.

## i18n RU/EN

- Словари находятся в `apps/mgmt-ui/web/src/i18n.ts` (`en` и `ru`).
- Используйте `t("key")` вместо хардкода строк.
- Переключатель языка — в сайдбаре, состояние хранится в `localStorage`.
- Тест покрытия ключей: `apps/mgmt-ui/web/src/__tests__/i18n.test.ts`.
- Добавление ключа: добавить в оба словаря и заменить использования на `t("...")`.

## Debugging UI (Windows)

- Devtools: в debug-сборке devtools открываются автоматически, плюс горячая клавиша `F12` (Tauri). В релизе — тихо.
- Логи и ошибки: все IPC-вызовы идут через `invokeSafe(cmd, payload)` и логируют ошибки в консоль devtools (`console.error({ cmd, payload, error })`). В UI есть зелёные тосты на успех и красный баннер на ошибку.
- Где смотреть: откройте devtools (F12) → Console. На кнопках состояние загрузки блокирует повторные клики.

### Команды IPC (имена и аргументы)

- `sim_state()` → текущее DTO состояния.
- `sim_lists()` → словари (техн. узлы, сегменты, фабрики).
- `sim_tick({ months: u32 })` → тикнуть N месяцев.
- `sim_tick_quarter()` → тикнуть квартал (3× по месяцу) с авто-сейвом.
- `sim_override({ ovr: { price_delta_frac?, rd_delta_cents?, capacity_request?, tapeout? } })`.
- `sim_plan_quarter()` → планировщик решений на квартал.
- `sim_campaign_reset({ which?: string })` → перезапуск кампании ("1990s" или путь к YAML).
- `sim_campaign_set_difficulty({ level: string })` → пресеты сложности.
- `sim_tutorial_state()` → шаги туториала и прогресс.
- `sim_save({ name?: string })` / `sim_list_saves()` / `sim_load({ save_id: number })`.
- `sim_set_autosave({ on: boolean })` — включить/выключить автосейв.
- `sim_export_campaign({ path: string, format?: "json"|"parquet" })` — dry‑run экспорт.
- `sim_balance_info()` / `sim_build_info()` — вспомогательные.

Единицы измерения: `price_delta_frac = ±0.05` для ±5%, денежные поля — в центах (`…_cents`), R&D — центы/мес (`i64`).

### Типичные ошибки

- Несовпадение имени команды или ключа аргумента (`save_id` vs `saveId`).
- Проценты vs доли (`5%` → `0.05`).
- Рубли/доллары vs центы — все DTO в центах.

### Сборка Windows

- Быстрая сборка UI: `just release-ui` (на Windows используйте `scripts/windows/build-ui.ps1`).
- Полный релиз: `just release-all` и упаковка `just package-win`.
