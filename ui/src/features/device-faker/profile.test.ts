import { describe, expect, it } from "vitest"
import { profileSnippet } from "./profile"

describe("profile candidates", () => {
  it("retains exact multi-user package keys and escapes TOML values", () => {
    const content = profileSnippet('my "phone"', "com.example.app@0\ncom.example.app@999", {
      model: 'A"\nB',
      brand: "",
    })
    expect(content).toContain(String.raw`[templates."my \"phone\""]`)
    expect(content).toContain('["com.example.app@0","com.example.app@999"]')
    expect(content).toContain('model = "A\\"\\nB"')
    expect(content).not.toContain("brand =")
  })
  it("creates whole direct records rather than implicit template inheritance", () => {
    const content = profileSnippet("", "com.example.a,com.example.b@0", { model: "Phone" })
    expect(content.match(/\[\[apps\]\]/g)).toHaveLength(2)
    expect(content.match(/model = "Phone"/g)).toHaveLength(2)
  })
  it("requires explicit targets and device fields", () => {
    expect(() => profileSnippet("", "", { model: "Phone" })).toThrow()
    expect(() => profileSnippet("", "com.example.a", { model: "" })).toThrow()
  })
})
