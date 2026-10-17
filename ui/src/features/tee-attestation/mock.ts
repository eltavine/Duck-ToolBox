import { MockFailure } from "@/core/mock"
import type { MockModule } from "@/core/mock"

const OUT = "/data/adb/duck-toolbox/var/outputs/tee-attestation-example"
const mock: MockModule = {
  feature: { id: "tee-attestation", contract: 1 },
  handlers: {
    "tee-attestation.status": () => ({
      mode: "offline",
      device_extraction: false,
      keybox_formats: ["auto", "xml", "pem"],
      seed_derivations: ["ed25519-seed", "p256-seed", "p256-scalar"],
      max_input_bytes: 262144,
      max_inline_bytes: 32768,
    }),
    "tee-attestation.keybox": (_args, _flags, input) => {
      const request = input as { source: { path?: string }; export: boolean }
      if (request.source.path?.includes("invalid"))
        throw new MockFailure("invalid_keybox", "Keybox verification failed")
      return {
        report: {
          input_format: "pem",
          source_sha256: "ab".repeat(32),
          keyboxes: 1,
          keys: [
            {
              algorithm: "ecdsa",
              certificates: 2,
              public_key_sha256: "cd".repeat(32),
              valid_now: true,
            },
          ],
          certificate_signatures_valid: true,
          trust_verified: false,
          revocation_checked: false,
        },
        artifacts: request.export
          ? { material_path: `${OUT}/keybox.xml`, report_path: `${OUT}/report.json` }
          : null,
      }
    },
    "tee-attestation.seed": (_args, _flags, input) => {
      const request = input as { source: { path?: string }; derivation: string; export: boolean }
      const matches = !request.source.path?.includes("mismatch")
      if (request.export && !matches)
        throw new MockFailure("identity_mismatch", "Nothing was exported")
      return {
        report: {
          derivation: request.derivation,
          curve: request.derivation === "ed25519-seed" ? "ed25519" : "p256",
          identity_matches: matches,
          csr_signature_valid: true,
          csr_sha256: "ab".repeat(32),
          public_key_sha256: "cd".repeat(32),
          trust_verified: false,
        },
        artifacts: request.export
          ? { material_path: `${OUT}/identity.bin`, report_path: `${OUT}/report.json` }
          : null,
      }
    },
  },
}
export default mock
