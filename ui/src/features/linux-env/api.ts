import { z } from "zod"
import { duckd } from "@/core/duckd"
const byteLength = (value: string) => new TextEncoder().encode(value).length
const uint = z.number().int().min(0).max(Number.MAX_SAFE_INTEGER)
export const idSchema = z.string().regex(/^[a-z][a-z0-9-]{0,31}$/)
export const configSchema = z
  .object({
    name: z
      .string()
      .trim()
      .min(1)
      .refine((v) => byteLength(v) <= 120 && !/\p{Cc}/u.test(v)),
    auto_start: z.boolean(),
    dns: z.array(z.union([z.ipv4(), z.ipv6()])).max(4),
    services: z
      .array(
        z.object({
          id: idSchema,
          command: z
            .string()
            .refine((v) => v.trim().length > 0 && byteLength(v) <= 8192 && !v.includes("\0")),
          enabled: z.boolean(),
        }),
      )
      .max(8),
  })
  .refine((v) => new Set(v.services.map((s) => s.id)).size === v.services.length)
export type Config = z.infer<typeof configSchema>
const digest = z.string().regex(/^[a-fA-F0-9]{64}$/)
const environment = z.object({
  id: idSchema,
  instance: digest,
  archive_sha256: digest,
  expanded_bytes: uint.max(16 * 2 ** 30),
  entries: uint.max(250000),
  created_at: uint,
  config: configSchema,
})
export const viewSchema = z.object({
  environment,
  revision: digest,
  phase: z.enum(["starting", "running", "stopping", "stopped", "failed", "unknown"]),
  message: z.string(),
  runtime: z.object({ config: configSchema }).nullable(),
})
export type EnvironmentView = z.infer<typeof viewSchema>
const job = z.object({
  token: digest,
  phase: z.enum(["queued", "copying", "validating", "extracting", "done", "failed"]),
  entries: uint.max(250000),
  bytes: uint,
  code: z.string(),
  message: z.string(),
})
const status = z.object({
  supported: z.boolean(),
  runtime_ready: z.boolean(),
  busy: z.boolean(),
  environments: z.array(viewSchema).max(16),
  job: job.nullable(),
})
export const inspectionSchema = z.object({
  path: z.string(),
  bytes: uint,
  sha256: z.string().regex(/^[a-f0-9]{64}$/),
})
export type Inspection = z.infer<typeof inspectionSchema>
const target = (v: EnvironmentView) => ({
  id: v.environment.id,
  revision: v.revision,
  confirmed: true,
})
export const linuxApi = {
  status: () => duckd(["linux-env", "status"], { schema: status }),
  inspect: (path: string) => duckd(["linux-env", "inspect", path], { schema: inspectionSchema }),
  import: (id: string, name: string, archive: Inspection) =>
    duckd(["linux-env", "import"], {
      input: {
        id,
        config: { name, auto_start: false, dns: [], services: [] },
        path: archive.path,
        sha256: archive.sha256,
        confirmed: true,
      },
      schema: job,
    }),
  action: (action: "start" | "stop" | "remove", v: EnvironmentView) =>
    duckd(["linux-env", action], { input: target(v), schema: z.boolean() }),
  save: (v: EnvironmentView, config: Config) =>
    duckd(["linux-env", "save"], { input: { target: target(v), config }, schema: environment }),
  cancel: (token: string) =>
    duckd(["linux-env", "cancel", "--token", token], { schema: z.boolean() }),
  logs: (id: string) => duckd(["linux-env", "logs", "--id", id], { schema: z.string() }),
}
