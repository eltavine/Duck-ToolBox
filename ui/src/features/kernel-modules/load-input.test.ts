import { describe, expect, it } from "vitest"

import { parameterIssue } from "./load-input"

describe("kernel module load parameters", () => {
  it.each([
    ["a", 4096],
    ["字", 1365],
    ["😀", 1024],
  ])("enforces the UTF-8 budget for %s", (character, count) => {
    expect(parameterIssue(String(character).repeat(Number(count)))).toBeNull()
    expect(parameterIssue(String(character).repeat(Number(count) + 1))).toBe("parametersTooLong")
  })

  it.each(["\0", "\n", "\t", "\u007f", "\u0085", "\u009f"])(
    "rejects control characters (%j)",
    (control) => expect(parameterIssue(`label=${control}`)).toBe("parametersControl"),
  )

  it("preserves empty input, quoting, spaces and non-control Unicode", () => {
    expect(parameterIssue("")).toBeNull()
    expect(parameterIssue('label="中文 value" debug=1')).toBeNull()
    expect(parameterIssue("label=\u2028")).toBeNull()
  })
})
