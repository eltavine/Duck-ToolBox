import { exec, isKsuWebui, pm, toast } from "kernelsu-alt"

import type { HostModule, InstalledPackage, RootBridge } from "./types"

interface KsuGlobal {
  moduleInfo?: () => string
  spawn?: (command: string, args: string, options: string, callback: string) => void
  exit?: () => void
}

function ksuGlobal(): KsuGlobal | undefined {
  return (globalThis as { ksu?: KsuGlobal }).ksu
}

/**
 * KernelSU WebUI host (also KernelSU Next, APatch and WebUI X, which expose the same `ksu`
 * object). Built on `kernelsu-alt`, a drop-in superset of the official `kernelsu` package
 * that falls back to `pm` when the manager has no package API.
 */
export function createKernelSuBridge(): RootBridge {
  let module: HostModule | null | undefined

  return {
    available: true,

    exec(command) {
      // `env` and `cwd` are deliberately never used: the manager exports env values
      // unquoted (WebViewInterface.processOptions), so callers put quoted values in the
      // command itself.
      const host = ksuGlobal()
      // The callback form of native exec still waits for the entire shell job. Spawn
      // enqueues it instead, so a network request cannot freeze the WebView.
      const spawn = host?.spawn
      if (typeof spawn !== "function") return exec(command)
      return new Promise((resolve, reject) => {
        const scope = globalThis as unknown as Record<string, unknown>
        const callback = `duck_command_${crypto.randomUUID().replaceAll("-", "")}`
        const stdout: string[] = []
        const stderr: string[] = []
        let cleanupTimer: ReturnType<typeof setTimeout> | undefined
        const cleanup = () => {
          clearTimeout(cleanupTimer)
          delete scope[callback]
        }
        // Install handlers before calling native code: launching can fail synchronously.
        scope[callback] = {
          stdout: { emit: (_event: string, line: string) => stdout.push(line) },
          stderr: { emit: (_event: string, line: string) => stderr.push(line) },
          emit(event: string, value: number | Error) {
            if (event === "exit") {
              resolve({
                errno: value as number,
                stdout: stdout.join("\n"),
                stderr: stderr.join("\n"),
              })
              // KernelSU posts a companion error after a nonzero exit. Keep the
              // channel until then, with bounded cleanup for hosts that only emit exit.
              if (value === 0) cleanup()
              else cleanupTimer = setTimeout(cleanup, 1000)
            } else if (event === "error") {
              cleanup()
              reject(value)
            }
          },
        }
        try {
          // Native spawn appends a space after command, even with no arguments. The
          // newline keeps that space off a possible here-document's closing delimiter.
          spawn.call(host, `${command}\n`, "[]", "{}", callback)
        } catch (error) {
          cleanup()
          reject(error)
        }
      })
    },

    toast(message) {
      toast(message)
    },

    hostModule(): HostModule | null {
      // The hosting module is fixed for this WebUI session. Reading it lists every
      // installed module through a synchronous native call.
      if (module !== undefined) return module
      module = null
      try {
        const raw = ksuGlobal()?.moduleInfo?.()
        if (!raw) return null
        const info = JSON.parse(raw) as { id?: unknown; moduleDir?: unknown }
        const dir = typeof info.moduleDir === "string" ? info.moduleDir : ""
        const id = typeof info.id === "string" ? info.id : dir.split("/").pop() || ""
        module = dir || id ? { id, dir: dir || `/data/adb/modules/${id}` } : null
      } catch {
        // Unsupported hosts keep the default module directory for this session.
      }
      return module
    },

    async listPackages(kind) {
      try {
        return await pm.listPackages(kind)
      } catch {
        return []
      }
    },

    async packagesInfo(packageNames, signal) {
      const result: InstalledPackage[] = []
      // The native method is synchronous despite the library's Promise API. Give the
      // WebView a turn before each bounded batch, including the first, so it can paint.
      for (let offset = 0; offset < packageNames.length; offset += 24) {
        signal?.throwIfAborted()
        await new Promise<void>((resolve) => setTimeout(resolve, 0))
        signal?.throwIfAborted()
        try {
          const infos = await pm.getPackagesInfo(packageNames.slice(offset, offset + 24))
          for (const info of infos) {
            if (typeof info?.packageName !== "string" || "error" in info) continue
            const userId = info.userId
            result.push({
              packageName: info.packageName,
              label: info.appLabel || info.packageName,
              system: Boolean(info.isSystem),
              ...(typeof userId === "number" &&
                Number.isSafeInteger(userId) &&
                userId >= 0 && { userId }),
            })
          }
        } catch {
          // Unsupported hosts keep package names; preserve any batches already loaded.
          break
        }
      }
      return result
    },

    iconUrl(packageName) {
      return `ksu://icon/${packageName}`
    },

    exit() {
      ksuGlobal()?.exit?.()
    },
  }
}

export function isKernelSuHost(): boolean {
  return isKsuWebui()
}
