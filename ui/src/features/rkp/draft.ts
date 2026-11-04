import type { Curve, DeviceInfo, Profile } from "./api"

export const DEFAULT_KDF_LABEL = "rkp_bcc_km"

/** The editable form of a profile: the key source is flattened so both modes keep input. */
export interface Draft {
  mode: "seed" | "hw-key"
  seed_hex: string
  hw_key_hex: string
  kdf_label: string
  curve: Curve
  device: Omit<DeviceInfo, "vbmeta_digest"> & { vbmeta_digest: string }
  fingerprint: string
  include_fingerprint: boolean
  server_url: string
  num_keys: number
  output_path: string
}

export function toDraft(profile: Profile): Draft {
  const source = profile.key_source
  return {
    mode: source.kind === "seed" ? "seed" : "hw-key",
    seed_hex: source.kind === "seed" ? source.seed_hex : "",
    hw_key_hex: source.kind === "hw-key" ? source.hw_key_hex : "",
    kdf_label: source.kind === "hw-key" ? source.kdf_label || DEFAULT_KDF_LABEL : DEFAULT_KDF_LABEL,
    curve: profile.curve,
    device: { ...profile.device, vbmeta_digest: profile.device.vbmeta_digest ?? "" },
    fingerprint: profile.fingerprint.value,
    include_fingerprint: profile.fingerprint.include_fingerprint,
    server_url: profile.server_url,
    num_keys: profile.num_keys,
    output_path: profile.output_path,
  }
}

export function toProfile(draft: Draft): Profile {
  const seed = draft.seed_hex.trim()
  const hwKey = draft.hw_key_hex.trim()
  const keySource: Profile["key_source"] =
    draft.mode === "seed"
      ? seed
        ? { kind: "seed", seed_hex: seed }
        : { kind: "unset" }
      : hwKey
        ? {
            kind: "hw-key",
            hw_key_hex: hwKey,
            kdf_label: draft.kdf_label.trim() || DEFAULT_KDF_LABEL,
          }
        : { kind: "unset" }

  const device = Object.fromEntries(
    Object.entries(draft.device).map(([key, value]) => [
      key,
      typeof value === "string" ? value.trim() : Number(value) || 0,
    ]),
  ) as Draft["device"]

  return {
    key_source: keySource,
    curve: draft.curve,
    device: { ...device, vbmeta_digest: device.vbmeta_digest || null },
    fingerprint: {
      value: draft.fingerprint.trim(),
      include_fingerprint: draft.include_fingerprint,
    },
    server_url: draft.server_url.trim(),
    num_keys: Math.max(1, Math.round(Number(draft.num_keys) || 1)),
    output_path: draft.output_path.trim(),
  }
}

/** A profile whose device section was never filled in still carries the generic defaults. */
export function hasGenericDevice(draft: Draft): boolean {
  return draft.device.brand === "generic" && draft.device.model === "default"
}

/** Copies what the device reports; key material and user settings stay untouched. */
export function applyDetected(draft: Draft, detected: Profile): Draft {
  return {
    ...draft,
    device: {
      ...draft.device,
      ...detected.device,
      vbmeta_digest: detected.device.vbmeta_digest ?? draft.device.vbmeta_digest,
      security_level: draft.device.security_level,
      dice_issuer: draft.device.dice_issuer,
      dice_subject: draft.device.dice_subject,
    },
    fingerprint: detected.fingerprint.value || draft.fingerprint,
    server_url: detected.server_url || draft.server_url,
  }
}

export const HEX_SEED = /^[0-9a-fA-F]{64}$/
export const HEX_HW_KEY = /^[0-9a-fA-F]{32}$/
