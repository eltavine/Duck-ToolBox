import type {
  ArtifactsData,
  BridgeStatus,
  CommandHistoryEntry,
  DeviceInfo,
  InfoData,
  KeyboxData,
  PathsInfo,
  ProvisionData,
  TrickyStoreKeyboxInstallData,
  VerifyData,
} from "@/lib/types"

export type UiMode = "seed" | "hw-key"
export type RkpWorkspaceId =
  | "profile"
  | "info"
  | "provision"
  | "keybox"
  | "verify"
  | "artifacts"

export interface UiDeviceInfo extends DeviceInfo {
  vbmeta_digest: string
}

export interface UiProfile {
  mode: UiMode
  seed_hex: string
  hw_key_hex: string
  kdf_label: string
  device: UiDeviceInfo
  fingerprint: string
  server_url: string
  num_keys: number
  output_path: string
}

export interface BusyState {
  load: boolean
  device: boolean
  save: boolean
  clear: boolean
  info: boolean
  provision: boolean
  keybox: boolean
  replaceTrickyStore: boolean
  verify: boolean
  artifacts: boolean
}

export interface RkpWorkbenchState {
  bridge: BridgeStatus
  profile: UiProfile
  paths: PathsInfo | null
  busy: BusyState
  history: CommandHistoryEntry[]
  infoResult: InfoData | null
  provisionResult: ProvisionData | null
  keyboxResult: KeyboxData | null
  verifyResult: VerifyData | null
  artifacts: ArtifactsData | null
  verifyPath: string
  lastError: string
  errorDialogText: string
  errorDialogOpen: boolean
  keyboxPreviewOpen: boolean
  trickyStorePromptOpen: boolean
  trickyStoreInstallResult: TrickyStoreKeyboxInstallData | null
  activeWorkspace: RkpWorkspaceId
}

export interface RkpWorkbenchActions {
  setWorkspace(workspace: RkpWorkspaceId): void
  loadProfile(): Promise<void>
  syncDeviceProfile(): Promise<void>
  saveProfile(): Promise<void>
  clearProfile(): Promise<void>
  reloadArtifacts(): Promise<void>
  runInfo(): Promise<void>
  runProvision(): Promise<void>
  runKeybox(): Promise<void>
  replaceTrickyStoreKeybox(): Promise<void>
  runVerify(): Promise<void>
  copyText(value: string): Promise<void>
  dismissErrorDialog(): void
  openKeyboxPreview(): void
  closeKeyboxPreview(): void
  openTrickyStorePrompt(): void
  closeTrickyStorePrompt(): void
}
