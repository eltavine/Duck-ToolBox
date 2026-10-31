import { z } from "zod"
import { duckd } from "@/core/duckd"

export const ruleSchema = z.object({
  package_name: z.string(),
  user_id: z.number().int().min(0).max(21473),
  locale: z.string(),
})
export type LanguageRule = z.infer<typeof ruleSchema>
const configSchema = z.object({ rules: z.array(ruleSchema) })
const statusSchema = z.object({
  config: configSchema.nullable(),
  config_error: z.string().nullable(),
  revision: z.string(),
  enabled: z.boolean(),
  payload_installed: z.boolean(),
  parent_active: z.boolean(),
  restart_required: z.boolean(),
})
export type LanguageStatus = z.infer<typeof statusSchema>
export const languageApi = {
  status: () => duckd(["misc", "language", "status"], { schema: statusSchema }),
  save: (revision: string, rules: LanguageRule[]) =>
    duckd(["misc", "language", "save"], {
      input: { revision, config: { rules } },
      schema: statusSchema,
    }),
  enable: (revision: string) =>
    duckd(["misc", "language", "enable"], {
      input: { revision },
      schema: statusSchema,
    }),
  disable: () =>
    duckd(["misc", "language", "disable"], {
      schema: z.object({ enabled: z.literal(false), restart_required: z.boolean() }),
    }),
}
