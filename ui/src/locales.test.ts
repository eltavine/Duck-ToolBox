import { readdirSync, readFileSync } from "node:fs"
import path from "node:path"

import { describe, expect, it } from "vitest"

type Messages = Record<string, unknown>

const root = path.dirname(new URL(import.meta.url).pathname)
/** Translated in full. Tricky Addon's `ta` group follows upstream, so it is not counted. */
const complete = [
  "zh-TW",
  "ja",
  "ko",
  "es-ES",
  "fr",
  "de",
  "it",
  "pt-BR",
  "ru",
  "ar",
  "vi",
  "id",
  "uk",
  "pl",
]
const directories = [
  "core/i18n/locales",
  ...readdirSync(path.join(root, "features")).map((feature) => `features/${feature}/locales`),
]

function keys(messages: Messages, prefix = ""): string[] {
  return Object.entries(messages).flatMap(([key, value]) =>
    typeof value === "object" && value !== null
      ? keys(value as Messages, `${prefix}${key}.`)
      : [`${prefix}${key}`],
  )
}

function load(directory: string): Map<string, Messages> {
  const files = readdirSync(path.join(root, directory)).filter((file) => file.endsWith(".json"))
  return new Map(
    files.map((file) => [
      file.replace(/\.json$/, ""),
      JSON.parse(readFileSync(path.join(root, directory, file), "utf8")) as Messages,
    ]),
  )
}

describe.each(directories)("messages in %s", (directory) => {
  const locales = load(directory)
  const english = new Set(keys(locales.get("en") ?? {}))

  it("has an English fallback", () => {
    expect(english.size).toBeGreaterThan(0)
  })

  it("keeps Simplified Chinese complete", () => {
    expect(new Set(keys(locales.get("zh-CN") ?? {}))).toEqual(english)
  })

  it.each(complete)("keeps %s complete", (locale) => {
    const own = (list: Iterable<string>) =>
      new Set([...list].filter((key) => !key.startsWith("ta.")))
    expect(own(keys(locales.get(locale) ?? {}))).toEqual(own(english))
  })

  it.each([...locales])("%s adds no keys English lacks", (_locale, messages) => {
    expect(keys(messages).filter((key) => !english.has(key))).toEqual([])
  })
})
