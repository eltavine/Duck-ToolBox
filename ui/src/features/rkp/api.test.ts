import { afterEach, describe, expect, it } from "vitest"

import { setBridge } from "@/core/bridge"

import type { ImportedDiagnostic, ImportedIssue, Profile } from "./api"
import { rkpApi } from "./api"
import mock from "./mock"

afterEach(() => {
  setBridge(null)
  mock.handlers["rkp.profile.clear"]!([], {}, undefined)
})

function detected(): Profile {
  return structuredClone(
    (mock.handlers["rkp.profile.detect"]!([], {}, undefined) as { profile: Profile }).profile,
  )
}

function respond(data: unknown, command: string) {
  setBridge({
    available: true,
    exec: async () => ({
      errno: 0,
      stdout: JSON.stringify({ ok: true, api: 1, command, data }),
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

describe("RKP signing response contracts", () => {
  it("checks a save against its original fingerprint even if the caller edits the profile while waiting", async () => {
    const input = detected()
    input.fingerprint = { value: "source/build", include_fingerprint: true }
    const shown = structuredClone(mock.handlers["rkp.profile.show"]!([], {}, undefined)) as {
      profile: Profile
    }
    shown.profile = structuredClone(input)
    let finish!: (result: { errno: number; stdout: string; stderr: string }) => void
    setBridge({
      available: true,
      exec: () =>
        new Promise((resolve) => {
          finish = resolve
        }),
      toast: () => undefined,
      hostModule: () => null,
      listPackages: async () => [],
      packagesInfo: async () => [],
      iconUrl: () => null,
      exit: () => undefined,
    })
    const pending = rkpApi.save(input)
    input.fingerprint.value = "edited/build"
    input.fingerprint.include_fingerprint = false
    finish({
      errno: 0,
      stdout: JSON.stringify({ ok: true, api: 1, command: "rkp.profile.save", data: shown }),
      stderr: "",
    })
    await expect(pending).resolves.toEqual(shown)
  })

  it.each([false, true])(
    "rejects a save response that silently changed fingerprint inclusion %s",
    async (include) => {
      const input = detected()
      input.fingerprint = { value: "source/build", include_fingerprint: include }
      const shown = structuredClone(mock.handlers["rkp.profile.show"]!([], {}, undefined)) as {
        profile: Profile
      }
      shown.profile = {
        ...input,
        fingerprint: { ...input.fingerprint, include_fingerprint: !include },
      }
      respond(shown, "rkp.profile.save")
      await expect(rkpApi.save(input)).rejects.toMatchObject({ code: "contract_mismatch" })
    },
  )

  it("accepts fingerprint whitespace normalization while rejecting another build", async () => {
    const input = detected()
    input.fingerprint.value = "  source/build  "
    const shown = structuredClone(mock.handlers["rkp.profile.show"]!([], {}, undefined)) as {
      profile: Profile
    }
    shown.profile = { ...input, fingerprint: { ...input.fingerprint, value: "source/build" } }
    respond(shown, "rkp.profile.save")
    await expect(rkpApi.save(input)).resolves.toEqual(shown)
    shown.profile.fingerprint.value = "other/build"
    respond(shown, "rkp.profile.save")
    await expect(rkpApi.save(input)).rejects.toMatchObject({ code: "contract_mismatch" })
  })

  it.each(["diagnose", "issue"] as const)(
    "rejects an invalid local verification report in a successful import %s response",
    async (action) => {
      for (const invalid of [
        { signature_valid: false },
        { cert_type: "widevine" },
        { keys_to_sign: 0 },
        { keys_to_sign: 2 },
        { version: 3 },
        { csr_version: 2 },
      ]) {
        const data = structuredClone(
          mock.handlers[`rkp.import.${action}`]!([], { bundle: "/tmp/material.json" }, undefined),
        ) as ImportedDiagnostic | ImportedIssue
        Object.assign(data.local_verify, { version: 1 }, invalid)
        respond(data, `rkp.import.${action}`)
        const input = {
          mode: "bundle" as const,
          bundlePath: "/tmp/material.json",
          chainPath: "",
          signingPath: "",
          configPath: "",
        }
        await expect(
          (action === "diagnose" ? rkpApi.importDiagnose : rkpApi.importIssue)(input),
        ).rejects.toMatchObject({ code: "contract_mismatch" })
      }
    },
  )

  it("rejects invalid software provisioning reports while allowing the supported key-count range", async () => {
    const input = detected()
    input.key_source = { kind: "seed", seed_hex: "11".repeat(32) }
    mock.handlers["rkp.profile.save"]!([], {}, input)
    for (const invalid of [
      { signature_valid: false },
      { cert_type: "widevine" },
      { keys_to_sign: 0 },
      { keys_to_sign: 17 },
      { keys_to_sign: 1.5 },
      { version: 3 },
      { csr_version: 2 },
    ]) {
      const data = structuredClone(mock.handlers["rkp.provision"]!([], {}, undefined)) as {
        local_verify: Record<string, unknown>
      }
      Object.assign(data.local_verify, { version: 1 }, invalid)
      respond(data, "rkp.provision")
      await expect(rkpApi.provision()).rejects.toMatchObject({ code: "contract_mismatch" })
    }
    for (const count of [1, 16]) {
      const data = structuredClone(mock.handlers["rkp.provision"]!([], {}, undefined)) as {
        local_verify: Record<string, unknown>
      }
      Object.assign(data.local_verify, { version: 1, keys_to_sign: count })
      respond(data, "rkp.provision")
      await expect(rkpApi.provision()).resolves.toEqual(data)
    }
  })
})
