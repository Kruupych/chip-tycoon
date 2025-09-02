import { dicts } from "../i18n";

test("ru dictionary covers all en keys", () => {
  const en = dicts.en as any;
  const ru = dicts.ru as any;
  for (const k of Object.keys(en)) {
    expect(typeof ru[k]).toBe("string");
  }
});

test("en dictionary covers all ru keys", () => {
  const en = dicts.en as any;
  const ru = dicts.ru as any;
  for (const k of Object.keys(ru)) {
    expect(typeof en[k]).toBe("string");
  }
});
