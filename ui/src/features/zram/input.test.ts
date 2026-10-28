import { describe, expect, it } from "vitest"
import { parsePlan } from "./input"
describe("ZRAM plan validation", () => {
  it("preserves automatic priority, zero disable and kernel-supported algorithm names", () => {
    expect(parsePlan("512", "-1", "lzo-rle", 65536, 4 * 1073741824)).toEqual({
      size_bytes: 536870912,
      priority: null,
      algorithm: "lzo-rle",
      memory_limit: 65536,
    })
    expect(parsePlan("0", "32767", "lz4", 0, 1073741824)?.size_bytes).toBe(0)
  })
  it.each([
    ["15", "1"],
    ["8193", "0"],
    ["1e3", "0"],
    ["512.5", "0"],
    ["512", "32768"],
    ["512", "-2"],
    ["", "1"],
    ["9007199254740992", "1"],
  ])("rejects invalid capacity %s or priority %s", (size, priority) => {
    expect(parsePlan(size!, priority!, "lz4", 0, 4 * 1073741824)).toBeNull()
  })
})
