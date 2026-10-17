import { z } from "zod"

import { duckd } from "@/core/duckd"

const deviceStatusSchema = z
  .object({
    prerequisites_met: z.boolean(),
    android_arm64: z.boolean(),
    root: z.boolean(),
    library_present: z.boolean(),
    device_present: z.boolean(),
    qualcomm_soc: z.boolean(),
    library_path: z.string(),
    device_path: z.string(),
    fingerprint: z.string(),
    soc_manufacturer: z.string(),
    soc_detection: z.object({
      state: z.enum(["qualcomm", "other", "unknown", "conflicting"]),
      model: z.string(),
      evidence: z.array(
        z.object({
          source: z.string(),
          value: z.string(),
          vendor: z.enum(["qualcomm", "other"]).nullable(),
        }),
      ),
    }),
    limitations: z.array(z.string()),
    blockers: z.array(z.string()),
  })
  .refine((data) => data.qualcomm_soc === (data.soc_detection.state === "qualcomm"), {
    message: "SoC identification disagrees with the Qualcomm prerequisite",
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

const provisionSchema = z.object({
  acknowledged_limitations: z.boolean(),
  limitations: z.array(z.string()),
  mode: z.string(),
  fingerprint: z.string(),
  challenge_hex: z.string(),
  csr_len: z.number(),
  local_verify: reportSchema,
  chain: chainSummarySchema,
  device_id: z.string(),
  keybox_xml: z.string(),
  artifacts: artifactPathsSchema,
  prerequisites: deviceStatusSchema,
})
export type Provision = z.infer<typeof provisionSchema>

export const rkpDeviceApi = {
  status: () => duckd(["rkp-device", "status"], { schema: deviceStatusSchema }),
  /**
   * Sends the acknowledgement the backend requires. It is the flag a user has to give,
   * not a default, so the button that reaches this call is always an explicit one.
   */
  provision: () =>
    duckd(["rkp-device", "provision", "--acknowledge-limitations"], { schema: provisionSchema }),
}
