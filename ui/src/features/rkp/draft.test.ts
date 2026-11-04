import { describe, expect, it } from "vitest"

import type { Profile } from "./api"
import { applyDetected, DEFAULT_KDF_LABEL, hasGenericDevice, toDraft, toProfile } from "./draft"

const profile: Profile = {
  key_source: { kind: "hw-key", hw_key_hex: "00".repeat(32), kdf_label: "" },
  curve: "p256",
  device: {
    brand: "generic",
    model: "default",
    device: "default",
    product: "default",
    manufacturer: "generic",
    fused: 1,
    vb_state: "green",
    os_version: "16",
    security_level: "tee",
    bootloader_state: "locked",
    boot_patch_level: 20260905,
    system_patch_level: 202609,
    vendor_patch_level: 20260905,
    vbmeta_digest: null,
    dice_issuer: "Android",
    dice_subject: "KeyMint",
  },
  fingerprint: {
    value: " google/caiman/caiman:16/BP3A/1:user/release-keys ",
    include_fingerprint: false,
  },
  server_url: "https://remoteprovisioning.googleapis.com/v1",
  num_keys: 1,
  output_path: "var/outputs/keybox.xml",
}

describe("RKP drafts", () => {
  it("preserves an enabled fingerprint policy when detecting device values", () => {
    const draft = { ...toDraft(profile), include_fingerprint: true }
    const detected = {
      ...profile,
      fingerprint: { value: "detected/build", include_fingerprint: false },
    }
    const next = applyDetected(draft, detected)
    expect(next.include_fingerprint).toBe(true)
    expect(toProfile(next).fingerprint).toEqual({
      value: "detected/build",
      include_fingerprint: true,
    })
  })
  it("round-trips a profile and fills the default KDF label", () => {
    const draft = toDraft(profile)
    expect(draft.kdf_label).toBe(DEFAULT_KDF_LABEL)
    const back = toProfile(draft)
    expect(back.key_source).toEqual({
      kind: "hw-key",
      hw_key_hex: "00".repeat(32),
      kdf_label: DEFAULT_KDF_LABEL,
    })
    expect(back.fingerprint.value).toBe("google/caiman/caiman:16/BP3A/1:user/release-keys")
    expect(back.device.vbmeta_digest).toBeNull()
  })

  it("keeps both key inputs while switching modes", () => {
    const draft = { ...toDraft(profile), mode: "seed" as const, seed_hex: "ab".repeat(32) }
    expect(toProfile(draft).key_source).toEqual({ kind: "seed", seed_hex: "ab".repeat(32) })
    expect(toProfile({ ...draft, seed_hex: " " }).key_source).toEqual({ kind: "unset" })
  })

  it("clamps the number of keys", () => {
    expect(toProfile({ ...toDraft(profile), num_keys: 0 }).num_keys).toBe(1)
  })

  it("applies detected device values but not key material", () => {
    const draft = toDraft(profile)
    expect(hasGenericDevice(draft)).toBe(true)
    const detected = {
      ...profile,
      key_source: { kind: "unset" as const },
      device: { ...profile.device, brand: "google", model: "Pixel 9 Pro" },
    }
    const next = applyDetected(draft, detected)
    expect(next.device.brand).toBe("google")
    expect(next.hw_key_hex).toBe(draft.hw_key_hex)
    expect(hasGenericDevice(next)).toBe(false)
  })
})
