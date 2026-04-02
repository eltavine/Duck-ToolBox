import type {
  BridgeStatus,
  CommandHistoryEntry,
  DeviceIdsProfileData,
  DeviceIdsProvisionData,
} from "@/lib/types"

export interface DeviceIdsBusyState {
  defaults: boolean
  provision: boolean
}

export interface DeviceIdsWorkbenchState {
  bridge: BridgeStatus
  profile: DeviceIdsProfileData
  busy: DeviceIdsBusyState
  history: CommandHistoryEntry[]
  result: DeviceIdsProvisionData | null
  lastError: string
  errorDialogText: string
  errorDialogOpen: boolean
}

export interface DeviceIdsWorkbenchActions {
  loadDefaults(): Promise<void>
  runProvision(): Promise<void>
  copyText(value: string): Promise<void>
  dismissErrorDialog(): void
}
