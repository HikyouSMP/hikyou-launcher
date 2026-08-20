import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import ja from "./locales/ja.json";
import en from "./locales/en.json";

i18n
  .use(initReactI18next)
  .init({
    lng: "ja",
    resources: { ja: { translation: ja }, en: { translation: en } },
    fallbackLng: "ja",
    interpolation: { escapeValue: false },
  });

export default i18n;
