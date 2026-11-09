// Minimal translation helper: a typed dictionary lookup with `{name}` style
// interpolation. The locale list is the single source for the language picker
// and for the text direction of the document, so adding a language means one
// entry here plus its dictionary file.
import { useCallback } from "react";

import { de } from "./locales/de";
import { en, type Dictionary } from "./locales/en";
import { es } from "./locales/es";
import { he } from "./locales/he";
import { ru } from "./locales/ru";
import { uk } from "./locales/uk";

export type Language = "ru" | "en" | "uk" | "he" | "de" | "es";

export interface LocaleInfo {
  code: Language;
  /** Name of the language in that language, so the picker needs no copy. */
  label: string;
  dir: "ltr" | "rtl";
}

export const LOCALES: LocaleInfo[] = [
  { code: "ru", label: "Русский", dir: "ltr" },
  { code: "uk", label: "Українська", dir: "ltr" },
  { code: "en", label: "English", dir: "ltr" },
  { code: "de", label: "Deutsch", dir: "ltr" },
  { code: "es", label: "Español", dir: "ltr" },
  { code: "he", label: "עברית", dir: "rtl" },
];

export const LANGUAGES: Language[] = LOCALES.map((locale) => locale.code);

const DICTIONARIES: Record<Language, Dictionary> = { ru, uk, en, de, es, he };

export type Translate = (
  key: keyof Dictionary,
  params?: Record<string, string | number>,
) => string;

export function isLanguage(value: string): value is Language {
  return LOCALES.some((locale) => locale.code === value);
}

/** Locale metadata for a stored language code, defaulting to the first one. */
export function localeFor(language: string): LocaleInfo {
  return LOCALES.find((locale) => locale.code === language) ?? LOCALES[0];
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
