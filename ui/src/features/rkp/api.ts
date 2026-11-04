import { z } from "zod"

import { DuckError, duckd } from "@/core/duckd"

const deviceSchema = z.object({
  brand: z.string(),
  model: z.string(),
  device: z.string(),
  product: z.string(),
  manufacturer: z.string(),
  fused: z.number(),
  vb_state: z.string(),
  os_version: z.string(),
  security_level: z.string(),
  bootloader_state: z.string(),
  boot_patch_level: z.number(),
  system_patch_level: z.number(),
  vendor_patch_level: z.number(),
  vbmeta_digest: z.string().nullable().optional(),
  dice_issuer: z.string(),
  dice_subject: z.string(),
})
export type DeviceInfo = z.infer<typeof deviceSchema>

const keySourceSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("unset") }),
  z.object({ kind: z.literal("seed"), seed_hex: z.string() }),
  z.object({ kind: z.literal("hw-key"), hw_key_hex: z.string(), kdf_label: z.string() }),
])

export const curves = ["ed25519", "p256"] as const
export type Curve = (typeof curves)[number]

const profileSchema = z.object({
  key_source: keySourceSchema,
  curve: z.enum(curves),
  device: deviceSchema,
  fingerprint: z.object({ value: z.string(), include_fingerprint: z.boolean() }),
  server_url: z.string(),
  num_keys: z.number(),
  output_path: z.string(),
})
export type Profile = z.infer<typeof profileSchema>

const pathsSchema = z.object({
  profile_path: z.string(),
  profile_secrets_path: z.string(),
  outputs_dir: z.string(),
})

const profileEnvelopeSchema = z.object({ profile: profileSchema, paths: pathsSchema })
const detectSchema = z.object({ profile: profileSchema })

const infoSchema = z.object({
  mode: z.string(),
  curve: z.string(),
  public_key_hex: z.string(),
  output_path: z.string(),
})
export type Info = z.infer<typeof infoSchema>

const chainSummarySchema = z.object({ certificates: z.number(), subjects: z.array(z.string()) })

const reportSchema = z.object({
  version: z.number(),
  dice_entries: z.number(),
  uds_pub_hex: z.string(),
  signature_valid: z.boolean(),
  csr_version: z.number(),
  cert_type: z.string(),
  brand: z.string().nullable().optional(),
  keys_to_sign: z.number(),
  unverified_fingerprint: z.string().nullable().optional(),
})
export type VerifyReport = z.infer<typeof reportSchema>

const signingReportSchema = reportSchema.extend({
  version: z.literal(1),
  csr_version: z.literal(3),
  cert_type: z.literal("keymint"),
  signature_valid: z.literal(true),
  keys_to_sign: z.number().int().min(1).max(16),
})
const importedReportSchema = signingReportSchema.extend({ keys_to_sign: z.literal(1) })

const provisionSchema = z.object({
  curve: z.string(),
  challenge_hex: z.string(),
  csr_path: z.string(),
  csr_len: z.number(),
  protected_data_len: z.number(),
  local_verify: signingReportSchema,
  cert_chains: z.array(
    z.object({ index: z.number(), path: z.string(), summary: chainSummarySchema }),
  ),
  local_test_mode: z.boolean(),
  fetch_eek_error: z.string().nullable().optional(),
  server_submission_error: z.string().nullable().optional(),
})
export type Provision = z.infer<typeof provisionSchema>

const keyboxSchema = z.object({
  csr_path: z.string(),
  keybox_path: z.string(),
  device_id: z.string(),
  chain_summary: chainSummarySchema,
})
export type Keybox = z.infer<typeof keyboxSchema>

const verifySchema = z.object({ path: z.string(), report: reportSchema })
export type Verify = z.infer<typeof verifySchema>

export interface ImportDraft {
  mode: "bundle" | "split"
  bundlePath: string
  chainPath: string
  signingPath: string
  configPath: string
  fingerprintPolicy?: "config" | "include" | "omit"
}

const materialSchema = z.object({
  material_type: z.string(),
  algorithm: z.string(),
  dice_entries: z.number(),
  chain_signatures_valid: z.literal(true),
  key_matches: z.literal(true),
  source_trust: z.literal("unverified"),
})
const importedValidationSchema = z.object({
  stage: z.literal("validated"),
  material: materialSchema,
  ready_for_issuance: z.boolean(),
})
const importedDiagnosticSchema = z.object({
  stage: z.literal("diagnostic"),
  material: materialSchema,
  csr_path: z.string(),
  local_verify: importedReportSchema,
  diagnostic_only: z.literal(true),
})
const importedIssueSchema = z.object({
  stage: z.literal("exported"),
  material: materialSchema,
  csr_path: z.string(),
  keybox_path: z.string(),
  device_id: z.string(),
  chain_summary: chainSummarySchema,
  local_verify: importedReportSchema,
  server_accepted: z.literal(true),
  certificates_verified: z.literal(true),
  keybox_exported: z.literal(true),
})
export type ImportedValidation = z.infer<typeof importedValidationSchema>
export type ImportedDiagnostic = z.infer<typeof importedDiagnosticSchema>
export type ImportedIssue = z.infer<typeof importedIssueSchema>
export type ImportResult = ImportedValidation | ImportedDiagnostic | ImportedIssue

export function hasImportFiles(input: ImportDraft): boolean {
  return input.mode === "bundle"
    ? !!input.bundlePath.trim()
    : !!input.chainPath.trim() && !!input.signingPath.trim()
}

function importArgs(action: string, input: ImportDraft): string[] {
  if (!hasImportFiles(input)) {
    throw new DuckError(
      "invalid_material_input",
      "material file paths are incomplete",
      "rkp.import",
    )
  }
  const source =
    input.mode === "bundle"
      ? ["--bundle", input.bundlePath.trim()]
      : ["--chain", input.chainPath.trim(), "--signing", input.signingPath.trim()]
  const config = input.configPath.trim() ? ["--config", input.configPath.trim()] : []
  const fingerprint =
    input.fingerprintPolicy === "include" || input.fingerprintPolicy === "omit"
      ? ["--include-fingerprint", String(input.fingerprintPolicy === "include")]
      : []
  return ["rkp", "import", action, ...source, ...config, ...fingerprint]
}

function withImportFingerprintPolicy<T extends { local_verify: VerifyReport }>(
  schema: z.ZodType<T>,
  input: ImportDraft,
) {
  const policy = input.fingerprintPolicy
  return schema.refine(
    (data) => {
      const fingerprint = data.local_verify.unverified_fingerprint
      if (policy === "include") return typeof fingerprint === "string" && !!fingerprint.trim()
      if (policy === "omit") return fingerprint === null
      return true
    },
    { message: "CSR metadata disagrees with the requested fingerprint policy" },
  )
}

export const rkpApi = {
  show: () => duckd(["rkp", "profile", "show"], { schema: profileEnvelopeSchema }),
  save: (profile: Profile) => {
    const fingerprint = profile.fingerprint.value.trim()
    const includeFingerprint = profile.fingerprint.include_fingerprint
    return duckd(["rkp", "profile", "save"], {
      input: profile,
      schema: profileEnvelopeSchema.refine(
        (data) =>
          data.profile.fingerprint.value === fingerprint &&
          data.profile.fingerprint.include_fingerprint === includeFingerprint,
        { message: "Saved profile disagrees with the requested fingerprint configuration" },
      ),
    })
  },
  clear: () => duckd(["rkp", "profile", "clear"]),
  detect: () => duckd(["rkp", "profile", "detect"], { schema: detectSchema }),
  info: () => duckd(["rkp", "info"], { schema: infoSchema }),
  provision: (offline = false) =>
    duckd(["rkp", "provision", ...(offline ? ["--offline"] : [])], { schema: provisionSchema }),
  keybox: () => duckd(["rkp", "keybox"], { schema: keyboxSchema }),
  verify: (path: string) => duckd(["rkp", "verify", path], { schema: verifySchema }),
  importValidate: async (input: ImportDraft) =>
    duckd(importArgs("validate", input), { schema: importedValidationSchema }),
  importDiagnose: async (input: ImportDraft) =>
    duckd(importArgs("diagnose", input), {
      schema: withImportFingerprintPolicy(importedDiagnosticSchema, input),
    }),
  importIssue: async (input: ImportDraft) =>
    duckd(importArgs("issue", input), {
      schema: withImportFingerprintPolicy(importedIssueSchema, input),
    }),
}
