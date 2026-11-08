import { z } from "zod"
import { duckd } from "@/core/duckd"
import { previewSchema } from "./api"
const provenanceSchema = previewSchema.shape.provenance

const profilesSchema = z.object({
  invalid_profiles: z.array(z.object({ id: z.string(), error: z.string() })).default([]),
  profiles: z.array(
    z.object({
      id: z.string(),
      name: z.string(),
      created: z.number(),
      revision: z.string(),
      provenance: provenanceSchema.nullable(),
    }),
  ),
})
const resultSchema = z.enum(["unknown", "pass", "fail", "omitted"])
const recordsSchema = z.object({
  records: z.array(
    z.object({
      id: z.string(),
      created: z.number(),
      app: z.string(),
      version: z.string(),
      note: z.string(),
      results: z.tuple([resultSchema, resultSchema, resultSchema]),
      revision: z.string(),
      model: z.string(),
      fingerprint: z.string(),
      enabled: z.boolean(),
      mode: z.enum(["full", "scripts-only"]),
      environment: z
        .object({
          system_sdk: z.number().nullable().optional(),
          gms_version: z.string().nullable().optional(),
          store_version: z.string().nullable().optional(),
          activation: z
            .object({ reboot_required: z.boolean(), process_restart_required: z.boolean() })
            .optional(),
        })
        .nullable()
        .optional(),
      report: z
        .object({
          labels: z.array(z.string()),
          licensing: z.string().nullable(),
          recognition: z.string().nullable(),
          sdk: z.number().nullable(),
        })
        .nullable()
        .optional(),
      installation_source: z.string().default(""),
    }),
  ),
  user_reported: z.literal(true),
  verdict_verified: z.literal(false),
})
export const inspectionSchema = z.object({
  revision: z.string(),
  read_error: z.string().nullable(),
  runtime_observed: z.boolean(),
  runtime_error: z.string().nullable().optional(),
  processes: z
    .array(
      z.object({
        process: z.string(),
        pid: z.number(),
        start: z.string(),
        boot_id: z.string(),
        revision: z.string(),
        property_hook: z.boolean(),
        private_requested: z.number().int().nonnegative().max(1024).default(0),
        private_matching: z.number().int().nonnegative().max(1024).default(0),
        observed_at: z.number().optional(),
        build: z.record(z.string(), z.string()),
        properties: z.record(z.string(), z.string()),
      }),
    )
    .default([]),
  verdict_verified: z.literal(false),
  rows: z.array(
    z.object({
      field: z.string(),
      property: z.string(),
      system: z.string().nullable(),
      saved_build: z.string().nullable(),
      saved_property: z.string().nullable(),
      observed: z.null(),
    }),
  ),
})
export type ReportedResult = z.infer<typeof resultSchema>
export type TestRecordInput = {
  app: string
  version: string
  note: string
  basic: ReportedResult
  device: ReportedResult
  strong: ReportedResult
  revision: string
  installation_source?: string
}
const command = (op: string) => ["play-integrity", op]
export const libraryApi = {
  profiles: () => duckd(command("profile-list"), { schema: profilesSchema }),
  saveProfile: (name: string, content: string, provenance?: z.infer<typeof provenanceSchema>) =>
    duckd(command("profile-save"), {
      input: { name, content, provenance },
      schema: profilesSchema,
    }),
  profilePreview: (id: string) =>
    duckd(command("profile-preview"), { input: { id }, schema: previewSchema }),
  deleteProfile: (id: string) =>
    duckd(command("profile-delete"), { input: { id }, schema: profilesSchema }),
  inspect: () => duckd(command("inspect"), { schema: inspectionSchema }),
  records: () => duckd(command("record-list"), { schema: recordsSchema }),
  saveRecord: (input: TestRecordInput) =>
    duckd(command("record-save"), { input, schema: recordsSchema }),
  importRecord: (report: unknown, revision: string, installation_source: string) =>
    duckd(command("record-import"), {
      input: { report, revision, installation_source },
      schema: recordsSchema,
    }),
  clearRecords: () => duckd(command("record-clear"), { schema: recordsSchema }),
}
