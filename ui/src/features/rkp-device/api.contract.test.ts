// @vitest-environment node
import { afterEach, describe, expect, it } from "vitest"

import { isDuckError } from "@/core/duckd"
import type { Host } from "@/core/testing/host"
import { CONTRACT, DEVICE_PROPS, hasDuckd, installHost } from "@/core/testing/host"

import { rkpDeviceApi } from "./api"

let host: Host | undefined
afterEach(() => host?.dispose())

describe.skipIf(!hasDuckd)("device RKP API against the real duckd", CONTRACT, () => {
  it("reports prerequisites without contacting KeyMint or a server", async () => {
    host = installHost({ props: DEVICE_PROPS })

    const status = await rkpDeviceApi.status()

    // The data must validate against the schema the page reads; a host is never the
    // Qualcomm arm64 target the feature needs, so the prerequisites stay unmet.
    expect(status.prerequisites_met).toBe(false)
    expect(status.blockers).toContain("android_arm64_required")
    expect(status.limitations).toEqual(["partial_device_support", "rkp_only"])
  })

  it("sends the acknowledgement and reports unmet device prerequisites", async () => {
    host = installHost({ props: DEVICE_PROPS })

    const error = await rkpDeviceApi.provision().catch((caught: unknown) => caught)

    // The API always sends the flag, so on a host the failure is the device gate, not a
    // usage error; the important part is that the envelope still matches the contract.
    expect(isDuckError(error)).toBe(true)
    expect(error).not.toMatchObject({ code: "contract_mismatch" })
    expect(error).toMatchObject({ code: "unsupported_device" })
  })

  it.each([
    {
      name: "QTI alias",
      props: { "ro.soc.manufacturer": " QTI ", "ro.soc.model": "SM8650" },
      state: "qualcomm",
      blocker: null,
    },
    {
      name: "hardware fallback",
      props: { "ro.hardware": "qcom" },
      state: "qualcomm",
      blocker: null,
    },
    {
      name: "part-number fallback",
      props: { "ro.vendor.qti.soc_model": "SM8650Q-AB" },
      state: "qualcomm",
      blocker: null,
    },
    { name: "empty properties", props: {}, state: "unknown", blocker: "soc_vendor_unknown" },
    {
      name: "substring lookalike",
      props: { "ro.soc.manufacturer": "not Qualcomm" },
      state: "unknown",
      blocker: "soc_vendor_unknown",
    },
    {
      name: "other vendor",
      props: { "ro.soc.manufacturer": "MediaTek" },
      state: "other",
      blocker: "qualcomm_soc_required",
    },
    {
      name: "conflicting sources",
      props: { "ro.soc.manufacturer": "MediaTek", "ro.hardware": "qcom" },
      state: "conflicting",
      blocker: "soc_vendor_conflict",
    },
  ])("identifies $name through the real wrapper", async ({ props, state, blocker }) => {
    host = installHost({ props: props as Record<string, string> })
    const status = await rkpDeviceApi.status()
    expect(status.soc_detection.state).toBe(state)
    expect(status.qualcomm_soc).toBe(state === "qualcomm")
    const socBlockers = status.blockers.filter((item) =>
      ["qualcomm_soc_required", "soc_vendor_unknown", "soc_vendor_conflict"].includes(item),
    )
    expect(socBlockers).toEqual(blocker ? [blocker] : [])
  })

  it("uses kernel SoC information when vendor properties are absent", async () => {
    host = installHost({
      files: {
        "/sys/devices/soc0/family": "Snapdragon\n",
        "/sys/devices/soc0/machine": "SM8650\n",
      },
    })
    const status = await rkpDeviceApi.status()
    expect(status.soc_manufacturer).toBe("")
    expect(status.soc_detection).toMatchObject({
      state: "qualcomm",
      model: "SM8650",
      evidence: [
        { source: "/sys/devices/soc0/family", value: "Snapdragon", vendor: "qualcomm" },
        { source: "/sys/devices/soc0/machine", value: "SM8650", vendor: "qualcomm" },
      ],
    })
  })
})
