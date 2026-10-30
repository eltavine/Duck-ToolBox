/**
 * Runs the WebUI's real API functions against the real `duckd` binary: commands go through
 * the bridge, the module's `duckctl.sh` wrapper and the binary, exactly as on a device.
 * The device is a temporary sysroot (`DUCK_TOOLBOX_SYSROOT`) plus stand-ins for the
 * `getprop` and `pm` tools. Test-only; never imported by application code.
 */
import { spawn } from "node:child_process"
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs"
import { createServer } from "node:http"
import type { AddressInfo } from "node:net"
import { tmpdir } from "node:os"
import path from "node:path"

import { setBridge } from "@/core/bridge"

const repository = path.resolve(import.meta.dirname, "../../../..")

/** `DUCKD_BIN`, or the host debug build (`cargo build -p duckd`). */
export const duckdBinary = process.env.DUCKD_BIN ?? path.join(repository, "target/debug/duckd")
export const hasDuckd = existsSync(duckdBinary)
/** Each call spawns the wrapper and a debug build, which also generates RSA keys. */
export const CONTRACT = { timeout: 120_000 }
if (process.env.DUCKD_REQUIRED === "1" && !hasDuckd) {
  throw new Error(`contract tests need the duckd binary at ${duckdBinary}`)
}

export interface Device {
  /** Replaces the shipped module.prop. */
  moduleProp?: string
  props?: Record<string, string>
  userPackages?: string[]
  systemPackages?: string[]
  /** Files to create, by device path (`/data/adb/...`). */
  files?: Record<string, string>
}

export interface Host {
  /** Host path of a device path inside the fake root. */
  path(devicePath: string): string
  dispose(): void
}

const GETPROP = `#!/bin/sh
file="$(dirname "$0")/props.txt"
if [ "$#" -eq 0 ]; then
  while IFS='=' read -r key value; do printf '[%s]: [%s]\\n' "$key" "$value"; done < "$file"
else
  grep -m1 "^$1=" "$file" | cut -d= -f2-
fi
`

const PM = `#!/bin/sh
dir="$(dirname "$0")"
if [ "$1 $2" = "list packages" ]; then
  shift 2
  lists="user system"; paths=0
  for arg in "$@"; do
    case "$arg" in -3) lists=user ;; -s) lists=system ;; -f) paths=1 ;; esac
  done
  for list in $lists; do
    while read -r name; do
      [ -n "$name" ] || continue
      if [ "$paths" = 1 ]; then echo "package:/data/app/$name/base.apk=$name"; else echo "package:$name"; fi
    done < "$dir/$list.txt"
  done
elif [ "$1" = "path" ]; then
  echo "package:/data/app/$2/base.apk"
else
  exit 1
fi
`

function write(file: string, contents: string, mode?: number) {
  mkdirSync(path.dirname(file), { recursive: true })
  writeFileSync(file, contents)
  if (mode !== undefined) chmodSync(file, mode)
}

export function installHost(device: Device = {}): Host {
  const base = mkdtempSync(path.join(tmpdir(), "duck-contract-"))
  const module = path.join(base, "module")
  const sysroot = path.join(base, "sys")
  const tools = path.join(base, "tools")

  mkdirSync(path.join(module, "bin"), { recursive: true })
  mkdirSync(path.join(module, "webroot"), { recursive: true })
  // The wrapper as shipped, except for the interpreter: hosts have no /system/bin/sh.
  const wrapper = readFileSync(path.join(repository, "module/bin/duckctl.sh"), "utf8")
  write(
    path.join(module, "bin/duckctl.sh"),
    wrapper.replace(/^#!\/system\/bin\/sh/, "#!/bin/sh"),
    0o755,
  )
  if (device.moduleProp === undefined) {
    copyFileSync(path.join(repository, "module/module.prop"), path.join(module, "module.prop"))
  } else {
    write(path.join(module, "module.prop"), device.moduleProp)
  }
  symlinkSync(duckdBinary, path.join(module, "bin/duckd"))
  const engine =
    process.env.PIF_ENGINE_BIN ?? path.join(repository, "target/pif-engine/debug/pif-engine")
  if (existsSync(engine)) {
    mkdirSync(path.join(module, "play-integrity"), { recursive: true })
    symlinkSync(engine, path.join(module, "play-integrity/pif-engine"))
  }

  const faker =
    process.env.DEVICE_FAKER_ENGINE_BIN ??
    path.join(repository, "target/device-faker-manager/debug/device-faker-engine")
  if (existsSync(faker)) {
    mkdirSync(path.join(module, "device-faker"), { recursive: true })
    symlinkSync(faker, path.join(module, "device-faker/device-faker-engine"))
  }

  const props = Object.entries(device.props ?? {}).map(([key, value]) => `${key}=${value}\n`)
  write(path.join(tools, "props.txt"), props.join(""))
  write(path.join(tools, "user.txt"), (device.userPackages ?? []).join("\n") + "\n")
  write(path.join(tools, "system.txt"), (device.systemPackages ?? []).join("\n") + "\n")
  write(path.join(tools, "getprop"), GETPROP, 0o755)
  write(path.join(tools, "pm"), PM, 0o755)
  mkdirSync(path.join(sysroot, "data/adb/modules"), { recursive: true })
  for (const [devicePath, contents] of Object.entries(device.files ?? {})) {
    write(path.join(sysroot, devicePath), contents)
  }

  const env = {
    ...process.env,
    PATH: `${tools}:${process.env.PATH ?? ""}`,
    DUCK_TOOLBOX_SYSROOT: sysroot,
    DUCK_TOOLBOX_BUSYBOX_REEXEC: "1",
  }
  setBridge({
    available: true,
    // Asynchronous, so a server started by the same test keeps answering duckd.
    exec: (command) =>
      new Promise((resolve) => {
        const child = spawn("sh", ["-c", command], { env, timeout: 120_000 })
        let stdout = ""
        let stderr = ""
        child.stdout.setEncoding("utf8").on("data", (chunk: string) => (stdout += chunk))
        child.stderr.setEncoding("utf8").on("data", (chunk: string) => (stderr += chunk))
        child.on("close", (code) => resolve({ errno: code ?? 1, stdout, stderr }))
      }),
    toast: () => undefined,
    hostModule: () => ({ id: "duck-toolbox", dir: module }),
    listPackages: async () => [],
    packagesInfo: async () => [],
    iconUrl: () => null,
    exit: () => undefined,
  })

  return {
    path: (devicePath) => path.join(sysroot, devicePath),
    dispose: () => {
      setBridge(null)
      rmSync(base, { recursive: true, force: true })
    },
  }
}

export const DEVICE_PROPS: Record<string, string> = {
  "ro.product.brand": "google",
  "ro.product.model": "Pixel 9 Pro",
  "ro.product.device": "caiman",
  "ro.product.name": "caiman",
  "ro.product.manufacturer": "Google",
  "ro.build.version.release": "16",
  "ro.build.version.sdk": "36",
  "ro.build.version.security_patch": "2026-09-05",
  "ro.vendor.build.security_patch": "2026-09-05",
  "ro.vendor.boot_security_patch": "2026-09-05",
  "ro.build.fingerprint": "google/caiman/caiman:16/BP3A.250905.014/1:user/release-keys",
  "ro.product.cpu.abi": "arm64-v8a",
  "ro.serialno": "39021FDH2000AB",
  "ro.boot.vbmeta.digest": "a1".repeat(32),
  "ro.boot.verifiedbootstate": "green",
}

export interface Server {
  url(route: string): string
  close(): Promise<void>
}

/** A local HTTP server with fixed responses, standing in for GitHub or a keybox provider. */
export async function serve(routes: Record<string, string>): Promise<Server> {
  const server = createServer((request, response) => {
    const body = routes[request.url ?? ""]
    response.writeHead(body === undefined ? 404 : 200, { "content-type": "text/plain" })
    response.end(body ?? "not found")
  })
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve))
  const { port } = server.address() as AddressInfo
  return {
    url: (route) => `http://127.0.0.1:${port}${route}`,
    close: () => new Promise((resolve) => server.close(() => resolve())),
  }
}
