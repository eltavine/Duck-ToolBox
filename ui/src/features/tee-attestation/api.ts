import { z } from "zod"

import { duckd } from "@/core/duckd"

export type InputFile = { path: string } | { base64: string }
export type KeyboxFormat = "auto" | "xml" | "pem"
export type SeedFormat = "raw" | "hex"
export type Derivation = "ed25519-seed" | "p256-seed" | "p256-scalar"
export const MAX_UPLOAD = 32 * 1024

const artifacts = z.object({ material_path: z.string(), report_path: z.string() }).nullable()
const keyboxResult = z.object({
  report: z.object({
    input_format: z.enum(["xml", "pem"]),
    source_sha256: z.string(),
    keyboxes: z.number(),
    keys: z.array(
      z.object({
        algorithm: z.enum(["ecdsa", "rsa"]),
        certificates: z.number(),
        public_key_sha256: z.string(),
        valid_now: z.boolean(),
      }),
    ),
    certificate_signatures_valid: z.literal(true),
    trust_verified: z.literal(false),
    revocation_checked: z.literal(false),
  }),
  artifacts,
})
const seedResult = z.object({
  report: z.object({
    derivation: z.enum(["ed25519-seed", "p256-seed", "p256-scalar"]),
    curve: z.enum(["ed25519", "p256"]),
    identity_matches: z.boolean(),
    csr_signature_valid: z.literal(true),
    csr_sha256: z.string(),
    public_key_sha256: z.string(),
    trust_verified: z.literal(false),
  }),
  artifacts,
})

export type KeyboxResult = z.infer<typeof keyboxResult>
export type SeedResult = z.infer<typeof seedResult>

export const attestationApi = {
  status: () =>
    duckd(["tee-attestation", "status"], {
      schema: z.object({
        mode: z.literal("offline"),
        device_extraction: z.literal(false),
        keybox_formats: z.array(z.enum(["auto", "xml", "pem"])),
        seed_derivations: z.array(z.enum(["ed25519-seed", "p256-seed", "p256-scalar"])),
        max_input_bytes: z.number(),
        max_inline_bytes: z.number(),
      }),
    }),
  keybox: (input: { source: InputFile; format: KeyboxFormat; export: boolean }) =>
    duckd(["tee-attestation", "keybox"], { input, schema: keyboxResult }),
  seed: (input: {
    source: InputFile
    csr: InputFile
    format: SeedFormat
    derivation: Derivation
    offset?: number
    export: boolean
  }) => duckd(["tee-attestation", "seed"], { input, schema: seedResult }),
}
