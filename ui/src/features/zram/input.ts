import type { ZramConfig } from "./api"

function numericText(value: string): string {
  return value.trim().replace(/[٠-٩۰-۹０-９−]/gu, (char) => {
    if (char === "−") return "-"
    const code = char.codePointAt(0)!
    return String(code - (code >= 0xff10 ? 0xff10 : code >= 0x6f0 ? 0x6f0 : 0x660))
  })
}
export function capacityBytes(value: string, total: number): number | null {
  const text = numericText(value)
  if (!/^\d+$/.test(text)) return null
  const bytes = Number(text) * 1048576
  return Number.isSafeInteger(bytes) &&
    (bytes === 0 || (bytes >= 16 * 1048576 && bytes <= total * 2))
    ? bytes
    : null
}
export function swapPriority(value: string): number | null | undefined {
  const text = numericText(value)
  if (!/^-?\d+$/.test(text)) return undefined
  const priority = Number(text)
  if (!Number.isInteger(priority) || priority < -1 || priority > 32767) return undefined
  return priority === -1 ? null : priority
}
export function parsePlan(
  size: string,
  priority: string,
  algorithm: string,
  limit: number,
  total: number,
): ZramConfig | null {
  const size_bytes = capacityBytes(size, total),
    parsedPriority = swapPriority(priority)
  if (size_bytes === null || parsedPriority === undefined || !algorithm) return null
  return { size_bytes, algorithm, priority: parsedPriority, memory_limit: limit }
}
