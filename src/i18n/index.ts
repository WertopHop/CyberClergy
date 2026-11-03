// Minimal translation helper: a typed dictionary lookup with `{name}` style
// interpolation. A full i18n library is unnecessary while the interface only
// needs two locales.
import { useCallback } from "react";

import { en, type Dictionary } from "./locales/en";
import { ru } from "./locales/ru";

export type Language = "ru" | "en";

export const LANGUAGES: Language[] = ["ru", "en"];

const DICTIONARIES: Record<Language, Dictionary> = { en, ru };

export type Translate = (
  key: keyof Dictionary,
  params?: Record<string, string | number>,
) => string;

export function isLanguage(value: string): value is Language {
  return value === "ru" || value === "en";
}

export function dictionaryFor(language: string): Dictionary {
  return isLanguage(language) ? DICTIONARIES[language] : en;
}

export function useTranslate(language: string): Translate {
  return useCallback<Translate>(
    (key, params) => {
      const template = dictionaryFor(language)[key] ?? en[key] ?? key;
      if (!params) {
        return template;
      }
      return template.replace(/\{(\w+)\}/g, (match, name: string) =>
        name in params ? String(params[name]) : match,
      );
    },
    [language],
  );
}
