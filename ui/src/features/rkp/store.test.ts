import { createPinia, setActivePinia } from "pinia"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { rkpApi } from "./api"
import type { Profile, Verify } from "./api"
import { toDraft } from "./draft"
import { useRkpStore } from "./store"

const profile: Profile = {
  key_source: { kind: "seed", seed_hex: "ab".repeat(32) },
  curve: "ed25519",
  device: {
    brand: "google",
    model: "Pixel",
    device: "test",
    product: "test",
    manufacturer: "google",
    fused: 1,
    vb_state: "green",
    os_version: "16",
    security_level: "tee",
    bootloader_state: "locked",
    boot_patch_level: 20260905,
    system_patch_level: 202609,
    vendor_patch_level: 20260905,
    dice_issuer: "Android",
    dice_subject: "KeyMint",
  },
  fingerprint: { value: "test", include_fingerprint: false },
  server_url: "https://example.org",
  num_keys: 1,
  output_path: "/tmp/keybox.xml",
}
const shown = {
  profile,
  paths: {
    profile_path: "/tmp/profile.toml",
    profile_secrets_path: "/tmp/secrets.toml",
    outputs_dir: "/tmp",
  },
}
const info = { mode: "direct-seed", curve: "ed25519", public_key_hex: "00", output_path: "/tmp" }

beforeEach(() => setActivePinia(createPinia()))
afterEach(() => vi.restoreAllMocks())

describe("RKP operations", () => {
  it.each(["runProvision", "runKeybox"] as const)(
    "does not send %s when the fingerprint policy changes during profile saving",
    async (action) => {
      let finish!: (result: typeof shown) => void
      vi.spyOn(rkpApi, "save").mockImplementation(
        () =>
          new Promise((resolve) => {
            finish = resolve
          }),
      )
      const provision = vi
        .spyOn(rkpApi, "provision")
        .mockRejectedValue(new Error("stale request sent"))
      const keybox = vi.spyOn(rkpApi, "keybox").mockRejectedValue(new Error("stale request sent"))
      const store = useRkpStore()
      store.draft = { ...toDraft(profile), include_fingerprint: true }
      const pending = store[action]().catch((error: unknown) => error)
      store.draft.include_fingerprint = false
      finish({
        ...shown,
        profile: { ...profile, fingerprint: { ...profile.fingerprint, include_fingerprint: true } },
      })
      expect(await pending).toMatchObject({ code: "profile_changed" })
      expect(provision).not.toHaveBeenCalled()
      expect(keybox).not.toHaveBeenCalled()
      expect(store.draft.include_fingerprint).toBe(false)
      expect(store.profileBusy).toBe(false)
    },
  )
  it("prevents overlapping profile operations across tabs and releases the guard", async () => {
    let finish!: (result: typeof shown) => void
    vi.spyOn(rkpApi, "save").mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    const store = useRkpStore()
    store.draft = toDraft(profile)
    const pending = store.save()
    expect(store.profileBusy).toBe(true)
    await expect(store.runKeybox()).rejects.toMatchObject({ code: "operation_pending" })
    finish(shown)
    await pending
    expect(store.profileBusy).toBe(false)
  })

  it("preserves edits made during saving and does not publish a result for the older profile", async () => {
    let finish!: (result: typeof shown) => void
    vi.spyOn(rkpApi, "save").mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    const check = vi.spyOn(rkpApi, "info").mockResolvedValue(info)
    const store = useRkpStore()
    store.draft = toDraft(profile)
    const pending = store.checkKey()
    store.draft.fingerprint = "edited"
    finish(shown)
    await expect(pending).rejects.toMatchObject({ code: "profile_changed" })
    expect(store.draft.fingerprint).toBe("edited")
    expect(store.info).toBeNull()
    expect(check).not.toHaveBeenCalled()
  })

  it("discards a result if the profile changes while the command is running", async () => {
    let finish!: (result: typeof info) => void
    vi.spyOn(rkpApi, "save").mockResolvedValue(shown)
    const check = vi.spyOn(rkpApi, "info").mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    const store = useRkpStore()
    store.draft = toDraft(profile)
    const pending = store.checkKey()
    await vi.waitFor(() => expect(check).toHaveBeenCalledTimes(1))
    store.draft.fingerprint = "edited"
    finish(info)
    await pending
    expect(store.info).toBeNull()
  })

  it("does not refill an existing edited draft when the page loads again", async () => {
    const read = vi.spyOn(rkpApi, "show").mockResolvedValue(shown)
    const store = useRkpStore()
    store.draft = { ...toDraft(profile), fingerprint: "edited" }
    await store.load()
    expect(read).not.toHaveBeenCalled()
    expect(store.draft.fingerprint).toBe("edited")
  })

  it("does not show verification for a path that changed during the request", async () => {
    let finish!: (result: Verify) => void
    vi.spyOn(rkpApi, "verify").mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    const store = useRkpStore()
    store.verifyPath = "/tmp/first.cbor"
    const pending = store.runVerify()
    store.verifyPath = "/tmp/second.cbor"
    finish({
      path: "/tmp/first.cbor",
      report: {
        version: 3,
        dice_entries: 2,
        uds_pub_hex: "00",
        signature_valid: true,
        csr_version: 3,
        cert_type: "keymint",
        keys_to_sign: 1,
      },
    })
    await pending
    expect(store.verify).toBeNull()
    expect(store.profileBusy).toBe(false)
  })

  it("releases the shared guard after a backend failure", async () => {
    vi.spyOn(rkpApi, "save").mockRejectedValue(new Error("failed"))
    const store = useRkpStore()
    store.draft = toDraft(profile)
    await expect(store.save()).rejects.toThrow("failed")
    expect(store.profileBusy).toBe(false)
  })
})
