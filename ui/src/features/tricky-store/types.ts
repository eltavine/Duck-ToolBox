import type {
  BridgeStatus,
  CommandHistoryEntry,
  TrickyStoreFileListData,
  TrickyStoreKeyboxInstallData,
  TrickyStorePackageEntry,
  TrickyStoreStatusData,
  TrickyStoreTargetEntry,
  TrickyStoreTargetMode,
} from "@/lib/types"

export interface TrickyStoreBusyState {
  status: boolean
  save: boolean
  keybox: boolean
  files: boolean
}

export interface TrickyStoreWorkbenchState {
  bridge: BridgeStatus
  busy: TrickyStoreBusyState
  history: CommandHistoryEntry[]
  status: TrickyStoreStatusData | null
  packages: TrickyStorePackageEntry[]
  targets: TrickyStoreTargetEntry[]
  systemApps: string[]
  autoAddNewApps: boolean
  search: string
  keyboxSourcePath: string
  keyboxInstallResult: TrickyStoreKeyboxInstallData | null
  fileDialogOpen: boolean
  fileList: TrickyStoreFileListData | null
  filePath: string
  lastError: string
  errorDialogText: string
  errorDialogOpen: boolean
}

export interface TrickyStoreWorkbenchActions {
  refresh(): Promise<void>
  saveTargets(): Promise<void>
  installKeybox(): Promise<void>
  openFileDialog(): Promise<void>
  closeFileDialog(): void
  listFiles(path: string): Promise<void>
  chooseFile(path: string): void
  copyText(value: string): Promise<void>
  dismissErrorDialog(): void
  selectAllVisible(value: boolean): void
  togglePackage(packageName: string): void
  setMode(packageName: string, mode: TrickyStoreTargetMode): void
  addSystemApp(packageName: string): void
  removeSystemApp(packageName: string): void
}
