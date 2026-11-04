import { afterEach, describe, expect, it, vi } from "vitest"

import { bridge, setBridge } from "@/core/bridge"

import type { Protocol, Provision } from "./api"
import { rkpDeviceApi } from "./api"
import mock from "./mock"

const originalUrl = location.href
afterEach(() => {
  setBridge(null)
  history.replaceState(null, "", originalUrl)
})

function result(protocol: Protocol = "auto"): Provision {
  return mock.handlers["rkp-device.provision"]!(
    [],
    { protocol, "acknowledge-limitations": true },
    undefined,
  ) as Provision
}

function respond(data: Provision) {
  setBridge({
    available: true,
    exec: async () => ({
      errno: 0,
      stdout: JSON.stringify({ ok: true, api: 1, command: "rkp-device.provision", data }),
      stderr: "",
    }),
    toast: () => undefined,
    hostModule: () => null,
    listPackages: async () => [],
    packagesInfo: async () => [],
    iconUrl: () => null,
    exit: () => undefined,
  })
}

describe("device RKP successful-response validation", () => {
  it.each([false, true])(
    "passes fingerprint inclusion %s and safely quotes an explicit build",
    async (include) => {
      history.replaceState(null, "", "/")
      const data = result("v2")
      data.include_fingerprint = include
      data.fingerprint = "source/build'value"
      data.local_verify!.unverified_fingerprint = include ? data.fingerprint : null
      respond(data)
      const exec = vi.spyOn(bridge(), "exec")
      await rkpDeviceApi.provision("v2", include, "  source/build'value  ")
      expect(exec.mock.calls[0]![0]).toContain(`'--include-fingerprint' '${include}'`)
      expect(exec.mock.calls[0]![0]).toContain("'--fingerprint' 'source/build'\\''value'")
      exec.mockRestore()
    },
  )
  it("accepts MediaTek AIDL and rejects a claimed Qualcomm native capability", async () => {
    history.replaceState(null, "", "/?soc=mediatek")
    const data = result()
    respond(data)
    await expect(rkpDeviceApi.provision()).resolves.toEqual(data)
    data.prerequisites.v2_available = true
    respond(data)
    await expect(rkpDeviceApi.provision()).rejects.toMatchObject({ code: "contract_mismatch" })
  })

  it("rejects V2 metadata that disagrees with the reported policy", async () => {
    history.replaceState(null, "", "/")
    const data = result("v2")
    data.include_fingerprint = true
    respond(data)
    await expect(rkpDeviceApi.provision("v2", true)).rejects.toMatchObject({
      code: "contract_mismatch",
    })
  })

  it.each(["auto", "v1", "v2", "v1-native"] as const)(
    "accepts the verification semantics of %s",
    async (protocol) => {
      history.replaceState(null, "", "/")
      const data = result(protocol)
      respond(data)
      await expect(rkpDeviceApi.provision(protocol)).resolves.toEqual(data)
    },
  )

  it.each([{ signature_valid: false }, { cert_type: "widevine" }, { keys_to_sign: 2 }])(
    "rejects a V2 success report with %j",
    async (invalid) => {
      history.replaceState(null, "", "/")
      const data = result("v2")
      Object.assign(data.local_verify!, invalid)
      respond(data)
      await expect(rkpDeviceApi.provision("v2")).rejects.toMatchObject({
        code: "contract_mismatch",
      })
    },
  )

  it.each(["v2,v1", "v2"])("accepts ordered fallback after %s fail", async (failures) => {
    history.replaceState(null, "", `/?rkp_fail=${failures}`)
    const data = result()
    respond(data)
    await expect(rkpDeviceApi.provision()).resolves.toEqual(data)
  })

  it("accepts fallback that skips the unavailable AIDL adapter", async () => {
    history.replaceState(null, "", "/?rkp=v2-only&rkp_fail=v2")
    const data = result()
    respond(data)
    await expect(rkpDeviceApi.provision()).resolves.toEqual(data)
  })

  it("rejects reversed failures even when the final method and errors agree", async () => {
    history.replaceState(null, "", "/?rkp_fail=v2,v1")
    const data = result()
    data.attempts = [data.attempts[1]!, data.attempts[0]!, data.attempts[2]!]
    respond(data)
    await expect(rkpDeviceApi.provision()).rejects.toMatchObject({
      code: "contract_mismatch",
    })
  })
})
