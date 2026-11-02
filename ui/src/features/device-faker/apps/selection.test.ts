import { describe, expect, it } from "vitest"
import { appendPackageKeys, existingPackages } from "./selection"

describe("installed app selection", () => {
  it("keeps manual keys and never broadens an existing user-specific target", () => {
    const keys = "com.example.app@999, com.example.other@0\ncom.offline.app\n"
    expect(existingPackages(keys)).toEqual(
      new Set(["com.example.app", "com.example.other", "com.offline.app"]),
    )
    expect(appendPackageKeys(keys, ["com.example.app", "com.example.new", "com.example.new"])).toBe(
      keys + "com.example.new",
    )
  })
  it("separates additions from the final manual key and preserves selection order", () => {
    expect(appendPackageKeys("com.example.old", ["com.example.b", "com.example.a"])).toBe(
      "com.example.old\ncom.example.b\ncom.example.a",
    )
    expect(appendPackageKeys("", ["com.example.a"])).toBe("com.example.a")
  })
  it("leaves the exact input intact when nothing new is selected", () => {
    const keys = "  com.example.app@0,\ncom.example.app@999  "
    expect(appendPackageKeys(keys, [])).toBe(keys)
    expect(appendPackageKeys(keys, ["com.example.app"])).toBe(keys)
  })
})
