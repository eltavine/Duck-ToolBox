import { MockFailure } from "@/core/mock"
import type { MockModule } from "@/core/mock"
import type { ZramConfig, ZramDevice, ZramProfile } from "./api"
const scenario = new URLSearchParams(location.search).get("zram")
const bootId = "12345678-1234-1234-1234-123456789abc"
let device: ZramDevice = {
  name: "zram0",
  identity: "254:0",
  generation: "c".repeat(64),
  path: "/dev/block/zram0",
  size: 4 * 1073741824,
  initialized: true,
  algorithms: ["lzo-rle", "lz4", "zstd"],
  algorithm: "lz4",
  swap: {
    path: "/dev/block/zram0",
    kind: "partition",
    size: 4294963200,
    used: 536870912,
    priority: 100,
  },
  stats: { original: 536870912, compressed: 201326592, memory_used: 218103808, memory_limit: 0 },
  blocked: scenario === "mmd" ? ["mmd"] : scenario === "writeback" ? ["writeback"] : [],
  revision: "a".repeat(64),
}
if (scenario === "fractional") device.size += 4096
let profile: ZramProfile | null = null
let original: ZramConfig | null = null
function mutate(input: unknown) {
  const request = input as {
    confirmed: boolean
    revision: string
    action: string
    config?: ZramConfig
  }
  if (!request.confirmed) throw new MockFailure("confirmation_required", "Confirmation missing")
  if (scenario === "reject")
    throw new MockFailure("insufficient_memory", "Not enough RAM for swapoff")
  if (scenario === "timeout") throw new MockFailure("operation_timeout", "Outcome unknown")
  if (request.revision !== device.revision)
    throw new MockFailure("state_changed", "Configuration changed")
  const config = request.action === "restore" ? original : request.config
  if (!config) throw new MockFailure("original_unavailable", "No captured configuration")
  original ??= {
    size_bytes: device.size,
    algorithm: device.algorithm ?? "lz4",
    priority: device.swap?.priority ?? null,
    memory_limit: 0,
  }
  const before = { ...device.stats }
  device = {
    ...device,
    size: config.size_bytes,
    algorithm: config.algorithm,
    initialized: config.size_bytes > 0,
    swap: config.size_bytes
      ? {
          path: device.path!,
          kind: "partition",
          size: config.size_bytes - 4096,
          used: 0,
          priority: config.priority ?? -2,
        }
      : null,
    stats: { original: 0, compressed: 0, memory_used: 0, memory_limit: config.memory_limit },
    revision: device.revision === "a".repeat(64) ? "b".repeat(64) : "a".repeat(64),
  }
  return {
    device: device.name,
    action: request.action,
    before,
    after: device.stats,
    active: !!device.swap,
  }
}
const mock: MockModule = {
  feature: { id: "zram", contract: 2 },
  handlers: {
    "zram.status": () => ({
      operation_busy: scenario === "busy",
      operations_supported: scenario !== "unsupported",
      mmd_managed: scenario === "mmd",
      memory_total: 8 * 1073741824,
      memory_available: 3 * 1073741824,
      swap_in_pages: 18304,
      swap_out_pages: 32768,
      boot_id: bootId,
      devices: scenario === "empty" ? [] : [device],
      swaps: device.swap ? [device.swap] : [],
      warnings: [],
      saved: {
        version: 1,
        profiles: profile ? [profile] : [],
        originals: original
          ? [
              {
                device: device.name,
                generation: device.generation,
                boot_id: bootId,
                config: original,
              },
            ]
          : [],
        boot_results: [],
        attempted_boot: null,
      },
    }),
    "zram.apply": (_a, _f, input) => mutate(input),
    "zram.restore": (_a, _f, input) => mutate(input),
    "zram.save": (_a, _f, input) => {
      if (scenario === "save-failure")
        throw new MockFailure("zram_error", "Could not save the plan")
      profile = (input as { profile: ZramProfile }).profile
      return profile
    },
    "zram.forget": () => {
      profile = null
      return true
    },
  },
}
export default mock
