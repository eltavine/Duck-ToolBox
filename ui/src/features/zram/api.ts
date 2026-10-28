import { z } from "zod"

import { duckd } from "@/core/duckd"

const unsigned = z.number().int().min(0).max(Number.MAX_SAFE_INTEGER)
const configSchema = z.object({
  size_bytes: unsigned,
  algorithm: z.string(),
  priority: z.number().int().min(0).max(32767).nullable(),
  memory_limit: unsigned,
})
export type ZramConfig = z.infer<typeof configSchema>
const statsSchema = z.object({
  original: unsigned.nullable(),
  compressed: unsigned.nullable(),
  memory_used: unsigned.nullable(),
  memory_limit: unsigned.nullable(),
})
const swapSchema = z.object({
  path: z.string(),
  kind: z.string(),
  size: unsigned,
  used: unsigned,
  priority: z.number().int().min(-2147483648).max(32767),
})
const deviceSchema = z.object({
  name: z.string(),
  identity: z.string(),
  generation: z.string(),
  path: z.string().nullable(),
  size: unsigned,
  initialized: z.boolean(),
  algorithms: z.array(z.string()),
  algorithm: z.string().nullable(),
  swap: swapSchema.nullable(),
  stats: statsSchema,
  blocked: z.array(z.string()),
  revision: z.string(),
})
export type ZramDevice = z.infer<typeof deviceSchema>
const profileSchema = z.object({
  device: z.string(),
  config: configSchema,
  apply_at_boot: z.boolean(),
})
export type ZramProfile = z.infer<typeof profileSchema>
const statusSchema = z.object({
  operations_supported: z.boolean(),
  operation_busy: z.boolean(),
  mmd_managed: z.boolean().nullable(),
  memory_total: unsigned.positive(),
  memory_available: unsigned.nullable(),
  swap_in_pages: unsigned.nullable(),
  swap_out_pages: unsigned.nullable(),
  boot_id: z.string(),
  devices: z.array(deviceSchema),
  swaps: z.array(swapSchema),
  warnings: z.array(z.string()),
  saved: z.object({
    version: z.literal(1),
    profiles: z.array(profileSchema),
    originals: z.array(
      z.object({
        device: z.string(),
        generation: z.string().nullable(),
        boot_id: z.string(),
        config: configSchema,
      }),
    ),
    boot_results: z.array(
      z.object({ device: z.string(), boot_id: z.string(), code: z.string(), message: z.string() }),
    ),
    attempted_boot: z.string().nullable(),
  }),
})
const resultSchema = z.object({
  device: z.string(),
  action: z.enum(["apply", "restore"]),
  before: statsSchema,
  after: statsSchema,
  active: z.boolean(),
})
export type ZramResult = z.infer<typeof resultSchema>
export const zramApi = {
  status: () => duckd(["zram", "status"], { schema: statusSchema }),
  apply: (device: ZramDevice, config: ZramConfig) =>
    duckd(["zram", "apply"], {
      input: {
        device: device.name,
        revision: device.revision,
        confirmed: true,
        action: "apply",
        config,
      },
      schema: resultSchema,
    }),
  restore: (device: ZramDevice) =>
    duckd(["zram", "restore"], {
      input: { device: device.name, revision: device.revision, confirmed: true, action: "restore" },
      schema: resultSchema,
    }),
  save: (device: ZramDevice, config: ZramConfig, applyAtBoot: boolean) =>
    duckd(["zram", "save"], {
      input: {
        profile: { device: device.name, config, apply_at_boot: applyAtBoot },
        revision: device.revision,
        confirmed: true,
      },
      schema: profileSchema,
    }),
  forget: (device: string) => duckd(["zram", "forget", device], { schema: z.boolean() }),
}
