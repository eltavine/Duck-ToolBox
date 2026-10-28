import { describe, expect, it } from "vitest"
import { parsePlan, capacityBytes, swapPriority } from "./input"
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

describe("localized numeric input", () => {
  it.each(["١٠٢٤", "۱۰۲۴", "１０２４", " 1024 "])("accepts decimal digits in %s", (value) => {
    expect(capacityBytes(value, 4 * 1073741824)).toBe(1073741824)
  })
  it("accepts the minus sign for automatic priority and preserves validation per field", () => {
    expect(swapPriority("−١")).toBeNull()
    expect(swapPriority("32768")).toBeUndefined()
    expect(capacityBytes("4096.00390625", 8 * 1073741824)).toBeNull()
  })
})
