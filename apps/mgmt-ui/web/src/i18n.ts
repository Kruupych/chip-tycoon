type Dict = Record<string, string>;

const en: Dict = {
  app_title: "Mgmt",
  nav_dashboard: "Dashboard",
  nav_tutorial: "Tutorial",
  nav_campaign: "Campaign",
  nav_markets: "Markets",
  nav_rd: "R&D / Tapeout",
  nav_capacity: "Capacity",
  nav_ai: "AI Plan",
  btn_tick_month: "Tick Month",
  btn_sim_quarter: "Simulate Quarter",
  btn_sim_year: "Simulate Year",
  btn_save_load: "Save/Load…",
  hdr_active_mods: "Active Mods",
  lbl_difficulty: "Difficulty:",
  btn_export_report: "Export Report",
};

const ru: Dict = {
  app_title: "Управление",
  nav_dashboard: "Панель",
  nav_tutorial: "Туториал",
  nav_campaign: "Кампания",
  nav_markets: "Рынки",
  nav_rd: "R&D / Тейпаут",
  nav_capacity: "Мощности",
  nav_ai: "План ИИ",
  btn_tick_month: "Тик месяц",
  btn_sim_quarter: "Симулировать квартал",
  btn_sim_year: "Симулировать год",
  btn_save_load: "Сохранить/Загрузить…",
  hdr_active_mods: "Активные моды",
  lbl_difficulty: "Сложность:",
  btn_export_report: "Экспорт отчёта",
};

let lang: "en" | "ru" = (localStorage.getItem("lang") as any) || "en";

export function setLang(l: "en" | "ru") {
  lang = l;
  localStorage.setItem("lang", l);
}

export function getLang() {
  return lang;
}

export function t(key: string): string {
  const dict = lang === "ru" ? ru : en;
  return dict[key] || key;
}

export const dicts = { en, ru };

