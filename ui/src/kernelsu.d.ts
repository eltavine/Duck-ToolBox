declare module "kernelsu" {
  export interface PackageInfo {
    packageName: string
    versionName: string
    versionCode: number
    appLabel: string
    isSystem: boolean
    uid: number
  }

  export interface ExecOptions {
    cwd?: string
    env?: Record<string, string>
  }

  export interface ExecResult {
    errno?: number
    stdout: string
    stderr: string
  }

  export function exec(command: string, options?: ExecOptions): Promise<ExecResult>
  export function enableEdgeToEdge(enabled?: boolean): void
  export function getPackagesInfo(packages: string[]): PackageInfo[]
  export function listPackages(type: string): string[]
  export function moduleInfo(): string | Record<string, unknown> | null
  export function toast(message: string): void
}
