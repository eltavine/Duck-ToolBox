import type { ProfileData } from "@/lib/types"

import type { UiProfile } from "./types"

export const DEFAULT_KDF_LABEL = "rkp_bcc_km"

function mergeString(current: string, fallback: string) {
  return current.trim() ? current : fallback
}

function mergeNumber(current: number, fallback: number) {
  return current > 0 ? current : fallback
}

function normalizeText(value: string) {
  return value.trim()
}

function recommendedKdfLabel(profile: UiProfile) {
  if (profile.mode !== "hw-key") {
    return profile.kdf_label.trim()
  }

  return profile.kdf_label.trim() || DEFAULT_KDF_LABEL
}

export function normalizeUiProfile(profile: UiProfile): UiProfile {
  return {
    ...profile,
    seed_hex: normalizeText(profile.seed_hex),
    hw_key_hex: normalizeText(profile.hw_key_hex),
    kdf_label: recommendedKdfLabel(profile),
    fingerprint: normalizeText(profile.fingerprint),
    server_url: normalizeText(profile.server_url),
    num_keys: Math.max(1, Number(profile.num_keys || 1)),
    output_path: normalizeText(profile.output_path),
    device: {
      ...profile.device,
      brand: normalizeText(profile.device.brand),
      model: normalizeText(profile.device.model),
      device: normalizeText(profile.device.device),
      product: normalizeText(profile.device.product),
      manufacturer: normalizeText(profile.device.manufacturer),
      vb_state: normalizeText(profile.device.vb_state),
      os_version: normalizeText(profile.device.os_version),
      security_level: normalizeText(profile.device.security_level),
      bootloader_state: normalizeText(profile.device.bootloader_state),
      vbmeta_digest: normalizeText(profile.device.vbmeta_digest),
      dice_issuer: normalizeText(profile.device.dice_issuer),
      dice_subject: normalizeText(profile.device.dice_subject),
      fused: Number(profile.device.fused || 0),
      boot_patch_level: Number(profile.device.boot_patch_level || 0),
      system_patch_level: Number(profile.device.system_patch_level || 0),
      vendor_patch_level: Number(profile.device.vendor_patch_level || 0),
    },
  }
}

export function mergeMissingSystemProfile(
  profile: UiProfile,
  defaults: ProfileData,
): UiProfile {
  const current = normalizeUiProfile(profile)
  return normalizeUiProfile({
    ...current,
    fingerprint: mergeString(current.fingerprint, defaults.fingerprint.value),
    server_url: mergeString(current.server_url, defaults.server_url),
    num_keys: mergeNumber(current.num_keys, defaults.num_keys),
    output_path: mergeString(current.output_path, defaults.output_path),
    device: {
      ...current.device,
      brand: mergeString(current.device.brand, defaults.device.brand),
      model: mergeString(current.device.model, defaults.device.model),
      device: mergeString(current.device.device, defaults.device.device),
      product: mergeString(current.device.product, defaults.device.product),
      manufacturer: mergeString(
        current.device.manufacturer,
        defaults.device.manufacturer,
      ),
      vb_state: mergeString(current.device.vb_state, defaults.device.vb_state),
      os_version: mergeString(
        current.device.os_version,
        defaults.device.os_version,
      ),
      security_level: mergeString(
        current.device.security_level,
        defaults.device.security_level,
      ),
      bootloader_state: mergeString(
        current.device.bootloader_state,
        defaults.device.bootloader_state,
      ),
      vbmeta_digest: mergeString(
        current.device.vbmeta_digest,
        defaults.device.vbmeta_digest ?? "",
      ),
      dice_issuer: mergeString(
        current.device.dice_issuer,
        defaults.device.dice_issuer,
      ),
      dice_subject: mergeString(
        current.device.dice_subject,
        defaults.device.dice_subject,
      ),
      fused: mergeNumber(current.device.fused, defaults.device.fused),
      boot_patch_level: mergeNumber(
        current.device.boot_patch_level,
        defaults.device.boot_patch_level,
      ),
      system_patch_level: mergeNumber(
        current.device.system_patch_level,
        defaults.device.system_patch_level,
      ),
      vendor_patch_level: mergeNumber(
        current.device.vendor_patch_level,
        defaults.device.vendor_patch_level,
      ),
    },
  })
}

export function applyDetectedSystemProfile(
  profile: UiProfile,
  defaults: ProfileData,
): UiProfile {
  const current = mergeMissingSystemProfile(profile, defaults)
  return normalizeUiProfile({
    ...current,
    fingerprint: defaults.fingerprint.value || current.fingerprint,
    device: {
      ...current.device,
      brand: defaults.device.brand || current.device.brand,
      model: defaults.device.model || current.device.model,
      device: defaults.device.device || current.device.device,
      product: defaults.device.product || current.device.product,
      manufacturer: defaults.device.manufacturer || current.device.manufacturer,
      vb_state: defaults.device.vb_state || current.device.vb_state,
      os_version: defaults.device.os_version || current.device.os_version,
      bootloader_state:
        defaults.device.bootloader_state || current.device.bootloader_state,
      vbmeta_digest:
        defaults.device.vbmeta_digest?.trim() || current.device.vbmeta_digest,
      boot_patch_level:
        defaults.device.boot_patch_level || current.device.boot_patch_level,
      system_patch_level:
        defaults.device.system_patch_level || current.device.system_patch_level,
      vendor_patch_level:
        defaults.device.vendor_patch_level || current.device.vendor_patch_level,
    },
  })
}
