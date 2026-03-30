import { computed, reactive } from "vue"

import en from "./locales/en"
import zhCN from "./locales/zh-CN"

const STORAGE_KEY = "duck-toolbox/locale"
const messages = {
  en,
  "zh-CN": zhCN,
}

export type Locale = keyof typeof messages

const state = reactive({
  locale: detectLocale(),
})

function detectLocale(): Locale {
  const saved = globalThis.localStorage?.getItem(STORAGE_KEY)
  if (saved === "en" || saved === "zh-CN") {
    return saved
  }

  return globalThis.navigator?.language?.toLowerCase().startsWith("zh")
    ? "zh-CN"
    : "en"
}

function resolveKey(source: unknown, key: string): string | null {
  return key
    .split(".")
    .reduce<unknown>(
      (value, part) =>
        value && typeof value === "object" && part in value
          ? (value as Record<string, unknown>)[part]
          : null,
      source,
    ) as string | null
}

function interpolate(template: string, params?: Record<string, string | number>) {
  if (!params) {
    return template
  }

  return Object.entries(params).reduce(
    (value, [key, param]) => value.replaceAll(`{${key}}`, String(param)),
    template,
  )
}

export function translate(
  key: string,
  params?: Record<string, string | number>,
) {
  const message = resolveKey(messages[state.locale], key) ?? key
  return interpolate(message, params)
}

export function useI18n() {
  const locale = computed({
    get: () => state.locale,
    set: (value: Locale) => {
      state.locale = value
      globalThis.localStorage?.setItem(STORAGE_KEY, value)
    },
  })

  return {
    locale,
    locales: [
      { value: "zh-CN" as const, label: "CN" },
      { value: "en" as const, label: "EN" },
    ],
    t: translate,
  }
}
