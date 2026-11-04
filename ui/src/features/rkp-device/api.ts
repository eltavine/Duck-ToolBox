import { z } from "zod"

import { duckd } from "@/core/duckd"

const deviceStatusSchema = z
  .object({
    prerequisites_met: z.boolean(),
    android_arm64: z.boolean(),
    root: z.boolean(),
    library_present: z.boolean(),
    device_present: z.boolean(),
    v1_available: z.boolean(),
    v1_native_available: z.boolean(),
    v2_available: z.boolean(),
    aidl_apk: z.string().nullable(),
    qualcomm_soc: z.boolean(),
    mediatek_soc: z.boolean(),
    library_path: z.string(),
    device_path: z.string(),
    fingerprint: z.string(),
    soc_manufacturer: z.string(),
    soc_detection: z.object({
      state: z.enum(["qualcomm", "media_tek", "other", "unknown", "conflicting"]),
      model: z.string(),
      evidence: z.array(
        z.object({
          source: z.string(),
          value: z.string(),
          vendor: z.enum(["qualcomm", "media_tek", "other"]).nullable(),
        }),
      ),
    }),
    limitations: z.array(z.string()),
    blockers: z.array(z.string()),
  })
  .refine((data) => data.qualcomm_soc === (data.soc_detection.state === "qualcomm"), {
    message: "SoC identification disagrees with the Qualcomm prerequisite",
  })
  .refine((data) => data.mediatek_soc === (data.soc_detection.state === "media_tek"), {
    message: "SoC identification disagrees with the MediaTek prerequisite",
  })
  .refine((data) => data.qualcomm_soc || (!data.v2_available && !data.v1_native_available), {
    message: "Native RKP adapters require Qualcomm firmware",
  })
  .refine((data) => data.prerequisites_met === (data.blockers.length === 0), {
    message: "Prerequisite result disagrees with the reported blockers",
  })
export type DeviceStatus = z.infer<typeof deviceStatusSchema>

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

const chainSummarySchema = z.object({
  certificates: z.number(),
  subjects: z.array(z.string()),
})

const artifactPathsSchema = z.object({
  run_dir: z.string(),
  keybox_path: z.string(),
  csr_path: z.string(),
  cert_chain_path: z.string(),
  private_key_path: z.string(),
})
export type ArtifactPaths = z.infer<typeof artifactPathsSchema>

const provisionSchema = z
  .object({
    acknowledged_limitations: z.boolean(),
    limitations: z.array(z.string()),
    mode: z.string(),
    csr_protocol: z.enum(["v1", "v2"]),
    csr_backend: z.enum(["aidl", "native"]),
    attempts: z
      .array(
        z.object({
          method: z.enum(["v2", "v1", "v1-native"]),
          error: z.object({ code: z.string(), message: z.string() }).nullable(),
        }),
      )
      .min(1)
      .max(3),
    fingerprint: z.string(),
    challenge_hex: z.string(),
    include_fingerprint: z.boolean(),
    csr_len: z.number(),
    local_verify: reportSchema.nullable(),
    chain: chainSummarySchema,
    device_id: z.string(),
    keybox_xml: z.string(),
    artifacts: artifactPathsSchema,
    prerequisites: deviceStatusSchema,
  })
  .refine((data) => (data.csr_protocol === "v1") === (data.local_verify === null), {
    message: "CSR protocol disagrees with its local verification report",
  })
  .refine(
    (data) =>
      data.local_verify === null ||
      (data.local_verify.signature_valid &&
        data.local_verify.cert_type === "keymint" &&
        data.local_verify.keys_to_sign === 1),
    { message: "V2 issuance requires one KeyMint key with a valid signature" },
  )
  .refine((data) => data.csr_backend !== "aidl" || data.csr_protocol === "v1", {
    message: "The AIDL backend only implements V1",
  })
  .refine(
    (data) =>
      data.local_verify === null ||
      data.local_verify.unverified_fingerprint ===
        (data.include_fingerprint ? data.fingerprint : null),
    {
      message: "V2 CSR metadata disagrees with the reported fingerprint policy",
    },
  )
  .refine(
    (data) => {
      const last = data.attempts.at(-1)
      const expected =
        data.csr_protocol === "v2" ? "v2" : data.csr_backend === "aidl" ? "v1" : "v1-native"
      const order = { v2: 0, v1: 1, "v1-native": 2 }
      return (
        last?.method === expected &&
        last.error === null &&
        data.attempts.slice(0, -1).every((attempt) => attempt.error !== null) &&
        data.attempts.every(
          (attempt, index) =>
            index === 0 || order[data.attempts[index - 1]!.method] < order[attempt.method],
        )
      )
    },
    { message: "Attempt history disagrees with the successful issuance method" },
  )
export type Provision = z.infer<typeof provisionSchema>
export type Protocol = "auto" | "v1" | "v2" | "v1-native"

export const rkpDeviceApi = {
  status: () => duckd(["rkp-device", "status"], { schema: deviceStatusSchema }),
  /**
   * Sends the acknowledgement the backend requires. It is the flag a user has to give,
   * not a default, so the button that reaches this call is always an explicit one.
   */
  provision: (protocol: Protocol = "auto", includeFingerprint = false, fingerprint = "") => {
    const explicitFingerprint = fingerprint.trim()
    return duckd(
      [
        "rkp-device",
        "provision",
        "--acknowledge-limitations",
        "--protocol",
        protocol,
        "--include-fingerprint",
        String(includeFingerprint),
        ...(explicitFingerprint ? ["--fingerprint", explicitFingerprint] : []),
      ],
      {
        schema: provisionSchema.refine(
          (data) =>
            data.include_fingerprint === includeFingerprint &&
            (!explicitFingerprint || data.fingerprint === explicitFingerprint) &&
            (protocol === "auto" ||
              (data.attempts.length === 1 && data.attempts[0]?.method === protocol)),
          { message: "Issuance result disagrees with the requested fingerprint or protocol" },
        ),
      },
    )
  },
}
