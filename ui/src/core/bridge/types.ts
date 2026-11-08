/** Result of a root shell command, as the KernelSU `exec` API reports it. */
export interface ExecResult {
  errno: number
  stdout: string
  stderr: string
}

export interface InstalledPackage {
  packageName: string
  label: string
  system: boolean
  userId?: number
}

export interface HostModule {
  id: string
  /** Absolute module directory, e.g. `/data/adb/modules/duck-toolbox`. */
  dir: string
}

/**
 * What the WebUI needs from its host (KernelSU, APatch, WebUI X, ...). Everything else in
 * the app talks to the host only through this interface.
 */
export interface RootBridge {
  readonly available: boolean
  exec(command: string): Promise<ExecResult>
  toast(message: string): void
  /** The module whose WebUI is showing; may be another module linking to our webroot. */
  hostModule(): HostModule | null
  listPackages(kind: "user" | "system" | "all"): Promise<string[]>
  /**
   * Metadata for the given packages, with one entry per installed user when known.
   * Aborting stops queued batches, but an in-progress native call is synchronous.
   */
  packagesInfo(packageNames: string[], signal?: AbortSignal): Promise<InstalledPackage[]>
  /** URL that renders an app icon, or null when the host cannot serve icons. */
  iconUrl(packageName: string): string | null
  exit(): void
}
