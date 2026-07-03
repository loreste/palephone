import en from "@/locales/en.json";
import es from "@/locales/es.json";
import fr from "@/locales/fr.json";

export type Locale = "en" | "es" | "fr";

const locales: Record<Locale, Record<string, string>> = { en, es, fr };

const STORAGE_KEY = "pale.locale";

let currentLocale: Locale = (localStorage.getItem(STORAGE_KEY) as Locale) || "en";
let listeners: Array<() => void> = [];

export function getLocale(): Locale {
  return currentLocale;
}

export function setLocale(locale: Locale) {
  currentLocale = locale;
  localStorage.setItem(STORAGE_KEY, locale);
  listeners.forEach((fn) => fn());
}

export function onLocaleChange(fn: () => void): () => void {
  listeners.push(fn);
  return () => {
    listeners = listeners.filter((l) => l !== fn);
  };
}

/**
 * Translate a key to the current locale string.
 * Falls back to English, then to the key itself.
 */
export function t(key: string): string {
  return locales[currentLocale]?.[key] ?? locales.en?.[key] ?? key;
}

export const LOCALE_LABELS: Record<Locale, string> = {
  en: "English",
  es: "Espanol",
  fr: "Francais",
};
