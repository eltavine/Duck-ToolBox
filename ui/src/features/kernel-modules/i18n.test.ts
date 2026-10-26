// @vitest-environment node
import { readdirSync, readFileSync } from "node:fs"
import path from "node:path"

import { describe, expect, it } from "vitest"

const root = path.join(import.meta.dirname, "locales")
function flatten(value: Record<string, unknown>, prefix = ""): Record<string, string> {
  return Object.fromEntries(
    Object.entries(value).flatMap(([key, entry]) => {
      const name = prefix + key
      return typeof entry === "string"
        ? [[name, entry]]
        : Object.entries(flatten(entry as Record<string, unknown>, name + "."))
    }),
  )
}
const load = (locale: string) =>
  flatten(JSON.parse(readFileSync(path.join(root, locale), "utf8")) as Record<string, unknown>)
const english = load("en.json")
const parameters = (value: string) =>
  [...value.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort()

describe("kernel module translations", () => {
  it.each(readdirSync(root).filter((file) => file.endsWith(".json")))(
    "provides every message and preserves named parameters in %s",
    (file) => {
      const messages = load(file)
      expect(Object.keys(messages).sort()).toEqual(Object.keys(english).sort())
      for (const [key, value] of Object.entries(english)) {
        expect(messages[key]?.trim()).toBeTruthy()
        expect({ [key]: parameters(messages[key] ?? "") }).toEqual({ [key]: parameters(value) })
      }
    },
  )
})
