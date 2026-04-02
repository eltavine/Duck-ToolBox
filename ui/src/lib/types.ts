export type KeySource =
  | { kind: "unset" }
  | { kind: "seed"; seed_hex: string }
  | { kind: "hw-key"; hw_key_hex: string; kdf_label: string }

export interface DeviceInfo {
  brand: string
  model: string
  device: string
  product: string
  manufacturer: string
  fused: number
  vb_state: string
  os_version: string
  security_level: string
  bootloader_state: string
  boot_patch_level: number
  system_patch_level: number
  vendor_patch_level: number
  vbmeta_digest?: string | null
  dice_issuer: string
  dice_subject: string
}

export interface FingerprintConfig {
  value: string
}

export interface ProfileData {
  key_source: KeySource
  device: DeviceInfo
  fingerprint: FingerprintConfig
  server_url: string
  num_keys: number
  output_path: string
}

export interface PathsInfo {
  root: string
  data_root: string
  var_dir: string
  profile_path: string
  profile_secrets_path: string
  outputs_dir: string
  tmp_dir: string
  logs_dir: string
  log_path: string
  wrapper_path: string
  binary_path: string
}

export interface JsonError {
  code: string
  message: string
  details?: unknown
}

export interface Envelope<T> {
  ok: boolean
  command: string
  data: T | null
  error: JsonError | null
  ts?: number
}

export interface ProfileEnvelopeData {
  profile: ProfileData
  paths: PathsInfo
}

export interface InfoData {
  mode: string
  seed_hex: string
  ed25519_pubkey_hex: string
  device: DeviceInfo
  fingerprint: string
  server_url: string
  num_keys: number
  output_path: string
}

export interface ProvisionChain {
  index: number
  path: string
  summary: {
    certificates: number
    subjects: string[]
  }
}

export interface VerifyReport {
  version: number
  dice_entries: number
  uds_pub_hex: string
  signature_valid: boolean
  csr_version: number
  cert_type: string
  brand?: string | null
  keys_to_sign: number
}

export interface ProvisionData {
  mode: string
  cdi_leaf_pubkey_hex: string
  challenge_hex: string
  csr_path: string
  csr_len: number
  protected_data_len: number
  local_verify: VerifyReport
  cert_chains: ProvisionChain[]
}

export interface KeyboxData {
  mode: string
  cdi_leaf_pubkey_hex: string
  challenge_hex: string
  csr_path: string
  keybox_path: string
  keybox_xml: string
  device_id: string
  chain_summary: {
    certificates: number
    subjects: string[]
  }
}

export interface TrickyStoreKeyboxInstallData {
  source_path: string
  target_path: string
  backup_path: string | null
}

export interface VerifyData {
  path: string
  report: VerifyReport
}

export interface DeviceIdsProfileData {
  brand: string
  device: string
  product: string
  serial: string
  manufacturer: string
  model: string
  imei: string
  imei2: string
  meid: string
  meid2: string
  ta_name: string
  ta_path: string
  dry_run: boolean
}

export interface DeviceIdsProvisionedId {
  label: string
  value: string
}

export interface DeviceIdsProvisionData {
  count: number
  ids: DeviceIdsProvisionedId[]
  dry_run: boolean
  ta_name: string
  ta_path: string
  loaded_library: string | null
  ta_api_version: string | null
  ta_version: string | null
  report_path: string
}

export interface ArtifactFile {
  name: string
  path: string
  size: number
  modified_unix: number
}

export interface ArtifactsData {
  outputs: ArtifactFile[]
  profile_path: string
  profile_secrets_path: string
  log_path: string
}

export interface BridgeStatus {
  mode: "kernelsu" | "unavailable"
  moduleRoot: string
  dataRoot: string
  packageName: string | null
  versionName: string | null
  versionCode: number | null
}

export interface CommandHistoryEntry {
  command: string
  ok: boolean
  at: string
  message: string
}

export function defaultProfile(): ProfileData {
  return {
    key_source: { kind: "unset" },
    device: {
      brand: "",
      model: "",
      device: "",
      product: "",
      manufacturer: "",
      fused: 0,
      vb_state: "",
      os_version: "",
      security_level: "",
      bootloader_state: "",
      boot_patch_level: 0,
      system_patch_level: 0,
      vendor_patch_level: 0,
      vbmeta_digest: "",
      dice_issuer: "",
      dice_subject: "",
    },
    fingerprint: {
      value: "",
    },
    server_url: "https://remoteprovisioning.googleapis.com/v1",
    num_keys: 1,
    output_path: "var/outputs/keybox.xml",
  }
}
