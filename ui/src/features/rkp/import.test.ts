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

function response(data: unknown) {
  return {
    errno: 0,
    stdout: JSON.stringify({ ok: true, api: 1, command: "rkp.import.validate", data }),
    stderr: "",
  }
}

beforeEach(() => setActivePinia(createPinia()))
afterEach(() => setBridge(null))

describe("DICE material import", () => {
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
