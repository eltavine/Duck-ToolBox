import { z } from "zod"
import { duckd } from "@/core/duckd"

const summarySchema = z.object({
  templates: z.array(z.string()),
  targets: z.array(z.string()),
  warnings: z.array(
    z.object({
      code: z.enum(["overlap", "delete_requires_global", "pi_priority", "mapping_hide_ignored"]),
      target: z.string().optional(),
      count: z.number().optional(),
    }),
  ),
  global_effects: z.boolean(),
})
const configSchema = z.object({
  content: z.string(),
  revision: z.string(),
  summary: summarySchema.nullable(),
  error: z.string().nullable(),
})
const previewSchema = z.object({
  content: z.string(),
  revision: z.string(),
  summary: summarySchema,
})
const powerSchema = z.object({ enabled: z.boolean(), stop_revision: z.string() })
const statusSchema = powerSchema.extend({
  parent_active: z.boolean(),
  startup_ready: z.boolean(),
  startup_failed: z.boolean(),
  cleanup_pending: z.boolean(),
  global_recovery_requires_reboot: z.boolean().default(false),
  payload_installed: z.boolean(),
  payload_active: z.boolean(),
  payload_pending: z.boolean(),
  conflict: z.boolean(),
  injection_verified: z.literal(false),
  legacy_available: z.boolean(),
  config: configSchema,
  upstream_revision: z.string(),
})
export type Preview = z.infer<typeof previewSchema>
export const fakerApi = {
  status: () => duckd(["device-faker", "status"], { schema: statusSchema }),
  config: () => duckd(["device-faker", "config"], { schema: configSchema }),
  preview: (content: string) =>
    duckd(["device-faker", "preview"], { input: { content }, schema: previewSchema }),
  importPreview: () => duckd(["device-faker", "import-preview"], { schema: previewSchema }),
  apply: (preview: Preview, acknowledgeGlobal: boolean) =>
    duckd(["device-faker", "apply"], {
      input: {
        content: preview.content,
        revision: preview.revision,
        acknowledge_global: acknowledgeGlobal,
      },
      schema: configSchema,
    }),
  restorePreview: () => duckd(["device-faker", "restore-preview"], { schema: previewSchema }),
  restore: (revision: string, acknowledgeGlobal = false) =>
    duckd(["device-faker", "restore"], {
      input: { revision, acknowledge_global: acknowledgeGlobal },
      schema: configSchema,
    }),
  enable: (revision: string, stopRevision: string) =>
    duckd(["device-faker", "enable"], {
      input: { revision, stop_revision: stopRevision },
      schema: powerSchema,
    }),
  disable: () => duckd(["device-faker", "disable"], { schema: powerSchema }),
}
