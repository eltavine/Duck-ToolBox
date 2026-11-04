import { createPinia, setActivePinia } from "pinia"
import { afterEach, beforeEach, describe, expect, it } from "vitest"

import { setBridge } from "@/core/bridge"
import type { RootBridge } from "@/core/bridge"

import { rkpApi } from "./api"
import { useRkpStore } from "./store"

const material = {
  material_type: "dice-chain",
  algorithm: "ed25519",
  dice_entries: 2,
  chain_signatures_valid: true,
  key_matches: true,
  source_trust: "unverified",
}
const validated = { stage: "validated", material, ready_for_issuance: true }
const draft = {
  mode: "bundle" as const,
  bundlePath: " /data/local/tmp/dice.json ",
  chainPath: "",
  signingPath: "",
  configPath: " /data/local/tmp/request.json ",
}

function host(exec: RootBridge["exec"]): RootBridge {
  return {
    available: true,
    exec,
    toast: () => undefined,
    hostModule: () => null,
    listPackages: async () => [],
    packagesInfo: async () => [],
    iconUrl: () => null,
    exit: () => undefined,
  }
}

function response(data: unknown, command = "rkp.import.validate") {
  return {
    errno: 0,
    stdout: JSON.stringify({ ok: true, api: 1, command, data }),
    stderr: "",
  }
}

beforeEach(() => setActivePinia(createPinia()))
afterEach(() => setBridge(null))

describe("DICE material import", () => {
  it("keeps the requested fingerprint policy when the caller changes its draft during diagnosis", async () => {
    const input = { ...draft, fingerprintPolicy: "include" as "include" | "omit" }
    const data = {
      stage: "diagnostic",
      material,
      csr_path: "/tmp/request.cbor",
      local_verify: {
        version: 1,
        dice_entries: 2,
        uds_pub_hex: "00",
        signature_valid: true,
        csr_version: 3,
        cert_type: "keymint",
        keys_to_sign: 1,
        unverified_fingerprint: "source/build",
      },
      diagnostic_only: true,
    }
    let finish!: (result: ReturnType<typeof response>) => void
    setBridge(
      host(
        () =>
          new Promise((resolve) => {
            finish = resolve
          }),
      ),
    )
    const pending = rkpApi.importDiagnose(input)
    input.fingerprintPolicy = "omit"
    finish(response(data, "rkp.import.diagnose"))
    await expect(pending).resolves.toEqual(data)
  })
  it.each(["diagnose", "issue"] as const)(
    "accepts %s only when CSR metadata honors an explicit fingerprint policy",
    async (action) => {
      for (const fingerprintPolicy of ["include", "omit", "config"] as const) {
        for (const fingerprint of [null, "source/build"] as const) {
          const local_verify = {
            version: 1,
            dice_entries: 2,
            uds_pub_hex: "00",
            signature_valid: true,
            csr_version: 3,
            cert_type: "keymint",
            keys_to_sign: 1,
            unverified_fingerprint: fingerprint,
          }
          const data =
            action === "diagnose"
              ? {
                  stage: "diagnostic",
                  material,
                  csr_path: "/tmp/request.cbor",
                  local_verify,
                  diagnostic_only: true,
                }
              : {
                  stage: "exported",
                  material,
                  csr_path: "/tmp/request.cbor",
                  keybox_path: "/tmp/keybox.xml",
                  device_id: "synthetic",
                  chain_summary: { certificates: 2, subjects: [] },
                  local_verify,
                  server_accepted: true,
                  certificates_verified: true,
                  keybox_exported: true,
                }
          setBridge(host(async () => response(data, `rkp.import.${action}`)))
          const pending = (action === "diagnose" ? rkpApi.importDiagnose : rkpApi.importIssue)({
            ...draft,
            fingerprintPolicy,
          })
          const expected =
            fingerprintPolicy === "config" ||
            (fingerprintPolicy === "include") === (fingerprint !== null)
          const outcome = await pending.then(
            (value) => ({ data: value }),
            (error: unknown) => ({ error }),
          )
          expect(outcome).toMatchObject(
            expected ? { data } : { error: { code: "contract_mismatch" } },
          )
        }
      }
    },
  )
  it.each(["include", "omit"] as const)(
    "sends an explicit %s override without changing the default policy",
    async (fingerprintPolicy) => {
      let command = ""
      setBridge(host(async (value) => ((command = value), response(validated))))
      await rkpApi.importValidate({ ...draft, fingerprintPolicy })
      expect(command).toContain(`'--include-fingerprint' '${fingerprintPolicy === "include"}'`)
      await rkpApi.importValidate({ ...draft, fingerprintPolicy: "config" })
      expect(command).not.toContain("--include-fingerprint")
    },
  )

  it("invalidates readiness when the fingerprint policy changes", async () => {
    setBridge(host(async () => response(validated)))
    const store = useRkpStore()
    store.importDraft = { ...draft }
    await store.validateImport()
    expect(store.importReady).toBe(true)
    store.importDraft.fingerprintPolicy = "include"
    expect(store.importReady).toBe(false)
    expect(store.importResult).toBeNull()
  })
  it("passes only trimmed file paths to the backend without a shell heredoc", async () => {
    let command = ""
    setBridge(host(async (value) => ((command = value), response(validated))))
    expect(await rkpApi.importValidate(draft)).toEqual(validated)
    expect(command).toBe(
      "'/data/adb/modules/duck-toolbox/bin/duckctl.sh' 'rkp' 'import' 'validate' '--bundle' '/data/local/tmp/dice.json' '--config' '/data/local/tmp/request.json'",
    )
  })

  it("requires both split material files and rejects incomplete input before execution", async () => {
    let calls = 0
    setBridge(host(async () => (calls++, response(validated))))
    await expect(
      rkpApi.importValidate({ ...draft, mode: "split", chainPath: "/tmp/chain.cbor" }),
    ).rejects.toThrow()
    expect(calls).toBe(0)
  })

  it("clears validation and exports immediately when any imported input changes", async () => {
    setBridge(host(async () => response(validated)))
    const store = useRkpStore()
    store.importDraft = { ...draft }
    await store.validateImport()
    expect(store.importResult?.stage).toBe("validated")
    expect(store.importReady).toBe(true)
    store.importDraft.configPath = "/tmp/other.json"
    expect(store.importResult).toBeNull()
    expect(store.importReady).toBe(false)
  })

  it("prevents duplicate actions and discards a response for changed inputs", async () => {
    let finish!: (value: ReturnType<typeof response>) => void
    let calls = 0
    setBridge(host(() => (calls++, new Promise((resolve) => (finish = resolve)))))
    const store = useRkpStore()
    store.importDraft = { ...draft }
    const pending = store.validateImport()
    expect(store.importBusy).toBe(true)
    await store.validateImport()
    expect(calls).toBe(1)
    store.importDraft.bundlePath = "/tmp/changed.json"
    finish(response(validated))
    await pending
    expect(store.importResult).toBeNull()
    expect(store.importBusy).toBe(false)
  })

  it("requires a successful current validation before online issuance", async () => {
    let calls = 0
    setBridge(host(async () => (calls++, response(validated))))
    const store = useRkpStore()
    store.importDraft = { ...draft }
    await store.issueImport()
    expect(calls).toBe(0)
  })
})
