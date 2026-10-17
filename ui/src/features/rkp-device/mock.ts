import type { MockModule } from "@/core/mock"
import { MockFailure } from "@/core/mock"

import type { DeviceStatus } from "./api"

const OUT = "/data/adb/duck-toolbox/var/outputs/rkp-device-1750000000000"

function deviceStatus(): DeviceStatus {
  const status: DeviceStatus = {
    prerequisites_met: true,
    android_arm64: true,
    root: true,
    library_present: true,
    device_present: true,
    qualcomm_soc: true,
    library_path: "/vendor/lib64/libkeymasterdeviceutils.so",
    device_path: "/dev/smcinvoke",
    fingerprint: "google/husky/husky:15/AP3A.241005.015/1234567:user/release-keys",
    soc_manufacturer: "QTI",
    soc_detection: {
      state: "qualcomm",
      model: "SM8650",
      evidence: [
        { source: "ro.soc.manufacturer", value: "QTI", vendor: "qualcomm" },
        { source: "ro.hardware", value: "qcom", vendor: "qualcomm" },
      ],
    },
    limitations: ["partial_device_support", "rkp_only"],
    blockers: [],
  }
  const scenario = new URLSearchParams(location.search).get("soc")
  if (scenario === "unknown") {
    status.soc_manufacturer = ""
    status.soc_detection = { state: "unknown", model: "", evidence: [] }
  } else if (scenario === "fallback") {
    status.soc_manufacturer = ""
    status.soc_detection = {
      state: "qualcomm",
      model: "PINEAPPLE",
      evidence: [{ source: "/sys/devices/soc0/family", value: "Snapdragon", vendor: "qualcomm" }],
    }
  } else if (scenario === "other" || scenario === "conflict") {
    status.soc_manufacturer = "MediaTek"
    status.soc_detection = {
      state: scenario === "other" ? "other" : "conflicting",
      model: scenario === "other" ? "MT6989" : "SM8650",
      evidence: [{ source: "ro.soc.manufacturer", value: "MediaTek", vendor: "other" }],
    }
    if (scenario === "conflict")
      status.soc_detection.evidence.push({
        source: "ro.hardware",
        value: "qcom",
        vendor: "qualcomm",
      })
  }
  const blocker = {
    qualcomm: null,
    other: "qualcomm_soc_required",
    unknown: "soc_vendor_unknown",
    conflicting: "soc_vendor_conflict",
  }[status.soc_detection.state]
  status.qualcomm_soc = status.soc_detection.state === "qualcomm"
  status.blockers = blocker ? [blocker] : []
  status.prerequisites_met = status.blockers.length === 0
  return status
}

const mock: MockModule = {
  feature: { id: "rkp-device", contract: 2 },
  handlers: {
    "rkp-device.status": deviceStatus,
    "rkp-device.provision": (_args, flags) => {
      const status = deviceStatus()
      if (!status.prerequisites_met)
        throw new MockFailure("unsupported_device", "Prerequisites are missing")
      if (flags["acknowledge-limitations"] !== true) {
        throw new Error("acknowledgement is the only way this command runs")
      }
      return {
        acknowledged_limitations: true,
        limitations: status.limitations,
        mode: "device-ta-oracle",
        fingerprint: status.fingerprint,
        challenge_hex: "9f2c1d4b6a8e0f3c5b7d9a1e2f4c6b8d0a2e4f6c8b0d2e4f6a8c0e2f4b6d8a0c",
        csr_len: 1284,
        local_verify: {
          version: 1,
          dice_entries: 3,
          uds_pub_hex: "e63af59f0c1d2e3f40516a7b8c9d0e1f2a3b4c5d6e7f8091a2b3c4d5e6f70819",
          signature_valid: true,
          csr_version: 3,
          cert_type: "keymint",
          brand: "google",
          keys_to_sign: 1,
        },
        chain: {
          certificates: 4,
          subjects: ["CN=5f8e3c…,O=TEE", "CN=Droid CA3", "CN=Droid CA2", "CN=Key Attestation CA1"],
        },
        device_id: "Qualcomm-7c1f9a3b2d40",
        keybox_xml: '<?xml version="1.0"?>\n<AndroidAttestation>…</AndroidAttestation>',
        artifacts: {
          run_dir: OUT,
          keybox_path: `${OUT}/keybox.xml`,
          csr_path: `${OUT}/csr.cbor`,
          cert_chain_path: `${OUT}/cert_chain.der`,
          private_key_path: `${OUT}/private_key.pem`,
        },
        prerequisites: status,
      }
    },
  },
}

export default mock
