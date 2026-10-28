import type { ZramConfig } from "./api"
export function parsePlan(
  size: string,
  priority: string,
  algorithm: string,
  limit: number,
  total: number,
): ZramConfig | null {
  if (!/^\d+$/.test(size) || !/^-?\d+$/.test(priority)) return null
  const mib = Number(size),
    prio = Number(priority)
  const bytes = mib * 1048576
  if (!Number.isSafeInteger(bytes) || (mib !== 0 && (mib < 16 || bytes > total * 2))) return null
  if (!Number.isInteger(prio) || prio < -1 || prio > 32767 || !algorithm) return null
  return { size_bytes: bytes, algorithm, priority: prio === -1 ? null : prio, memory_limit: limit }
}
