import { describe, expect, it } from "vitest"
import { readDraft } from "./draft"
describe("draft restoration", () => {
  it("preserves a multilingual draft that exceeds the TOML byte limit", () => {
    const draft = { content: "设备".repeat(12000), revision: "saved-revision" }
    expect(readDraft(JSON.stringify(draft))).toEqual(draft)
  })
  it("discards malformed, wrongly typed or oversized cache entries", () => {
    for (const raw of [
      "broken",
      "null",
      "[]",
      '{"content":42,"revision":"x"}',
      JSON.stringify({ content: "x", revision: "x".repeat(129) }),
      JSON.stringify({ content: "x".repeat(256 * 1024 + 1), revision: "x" }),
    ]) {
      expect(readDraft(raw)).toBeNull()
    }
  })
})
