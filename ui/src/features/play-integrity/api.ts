import { z } from "zod"
import { duckd } from "@/core/duckd"

export const optionsSchema = z.object({
  enabled: z.boolean(),
  mode: z.enum(["full", "scripts-only"]),
  skipdelprop: z.boolean(),
  skippersistprop: z.boolean(),
  skipmatchdevice: z.boolean(),
})
export type Options = z.infer<typeof optionsSchema>
const configSchema = z.object({
  content: z.string(),
  json: z.string(),
  entries: z.record(z.string(), z.string()),
  revision: z.string(),
})
export const previewSchema = configSchema.extend({
  warnings: z
    .array(
      z.object({
        code: z.enum([
          "missing_api",
          "mismatch",
          "provider",
          "wildcard",
          "canary",
          "patch_scope",
          "vending_priority",
          "pixel_patch_unknown",
        ]),
        fields: z.array(z.string()),
      }),
    )
    .default([]),
  changes: z.array(
    z.object({ key: z.string(), before: z.string().nullable(), after: z.string().nullable() }),
  ),
  suggested_patch: z.string().nullable(),
  metadata: z
    .object({ released: z.string().nullable(), estimated_expiry: z.string().nullable() })
    .optional(),
  provenance: z
    .object({
      source: z.string(),
      device: z.string(),
      build: z.string(),
      fetched: z.number(),
      patch_known: z.boolean(),
      identity_bound: z.boolean().optional(),
      source_revision: z.string().optional(),
      channel: z.enum(["release", "beta", "canary"]).optional(),
      incremental: z.string().optional(),
      security_patch: z.string().optional(),
      vbmeta_digest: z.string().optional(),
      file_sha256: z.string().optional(),
      fingerprint: z.string().optional(),
      model: z.string().optional(),
      first_api_level: z.number().optional(),
    })
    .optional(),
})
export type Preview = z.infer<typeof previewSchema>
const statusSchema = z.object({
  app_replace_list: z.string(),
  upstream_revision: z.string(),
  options: optionsSchema,
  has_config: z.boolean(),
  zygisk_detected: z.boolean(),
  conflicts: z.array(z.string()),
  reboot_required: z.boolean(),
  process_restart_required: z.boolean().default(false),
  activation_reason: z.string().nullable().optional(),
  state: z.string(),
  data_dir: z.string(),
  verdict_verified: z.literal(false),
})
const taskSchema = z.object({
  id: z.string(),
  state: z.enum(["running", "completed", "failed", "cancelled"]),
  result: previewSchema.nullable(),
  error: z.string().nullable(),
  output: z.string().default(""),
})
export type Task = z.infer<typeof taskSchema>
const command = (operation: string) => ["play-integrity", operation]
const appliedSchema = z.object({
  config: configSchema,
  backup: z.string(),
  restart_required: z.boolean(),
})
const powerSchema = z.object({
  enabled: z.boolean(),
  disabled_by_marker: z.boolean(),
  stop_revision: z.string(),
  cleanup_pending: z.boolean(),
  issues: z.array(z.object({ step: z.string(), message: z.string() })),
})
export const integrityApi = {
  power: () => duckd(command("power"), { schema: powerSchema }),
  disable: () =>
    duckd(command("disable"), {
      schema: powerSchema.extend({
        reboot_required: z.boolean(),
        process_restart_required: z.boolean(),
      }),
    }),
  transactionStatus: () =>
    duckd(["integrity-transaction", "status"], {
      schema: z.object({ pending: z.boolean(), journal_path: z.string(), journal: z.unknown() }),
    }),
  applyTransaction: (
    preview: Preview,
    patch?: {
      target: string
      patch: string
      revision: string
      partitions?: Record<string, string> | null
    },
  ) =>
    duckd(["integrity-transaction", "apply"], {
      input: {
        engine: { content: preview.json, format: "json", revision: preview.revision },
        patch,
      },
      schema: appliedSchema,
    }),
  recoverTransaction: () =>
    duckd(["integrity-transaction", "recover"], {
      input: { confirmed: true },
      schema: z.object({ restored: z.literal(true), restart_required: z.boolean() }),
    }),
  status: () => duckd(command("status"), { schema: statusSchema }),
  config: () => duckd(command("config"), { schema: configSchema }),
  preview: (content: string, format = "auto", migrate = false) =>
    duckd(command("preview"), {
      input: { content, format, migrate, advanced: true },
      schema: previewSchema,
    }),
  importPreview: (source: string) =>
    duckd(command("import-preview"), { input: { source }, schema: previewSchema }),
  apply: (preview: Preview) =>
    duckd(command("apply"), {
      input: { content: preview.json, format: "json", revision: preview.revision },
      schema: z.object({ config: configSchema, backup: z.string(), restart_required: z.boolean() }),
    }),
  restore: (backup: string) =>
    duckd(command("restore"), {
      input: { backup },
      schema: z.object({ config: configSchema, restart_required: z.boolean() }),
    }),
  options: (
    options: Partial<Options> & { app_replace_list?: string; expected_stop_revision?: string },
  ) =>
    duckd(command("options"), {
      input: options,
      schema: z.object({ options: optionsSchema, reboot_required: z.boolean() }),
    }),
  taskStart: (strong: boolean, match_device: boolean) =>
    duckd(command("task-start"), { input: { strong, match_device }, schema: taskSchema }),
  taskStatus: (id: string) => duckd(command("task-status"), { input: { id }, schema: taskSchema }),
  taskCancel: (id: string) => duckd(command("task-cancel"), { input: { id }, schema: taskSchema }),
  restart: () =>
    duckd(command("restart"), {
      schema: z.object({
        processes: z.array(
          z.object({ process: z.string(), exit_code: z.number().nullable(), output: z.string() }),
        ),
        verdict_verified: z.literal(false),
      }),
    }),
  diagnose: () =>
    duckd(command("diagnose"), {
      schema: z.object({ log: z.string(), verdict_verified: z.literal(false) }),
    }),
}
