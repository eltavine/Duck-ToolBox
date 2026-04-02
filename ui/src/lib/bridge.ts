import { exec, getPackagesInfo, listPackages, moduleInfo, toast } from "kernelsu"

import { translate } from "@/i18n"
import type {
  ArtifactsData,
  BridgeStatus,
  CommandHistoryEntry,
  DeviceIdsProfileData,
  DeviceIdsProvisionData,
  DeviceInfo,
  Envelope,
  InfoData,
  KeyboxData,
  PathsInfo,
  ProfileData,
  ProfileEnvelopeData,
  ProvisionData,
  TrickyStoreKeyboxInstallData,
  VerifyData,
} from "@/lib/types"
import { defaultProfile } from "@/lib/types"

const MODULE_ROOT_FALLBACK = "/data/adb/modules/duck-toolbox"
const DATA_ROOT_FALLBACK = "/data/adb/duck-toolbox"
const TRICKY_STORE_KEYBOX_PATH = "/data/adb/tricky_store/keybox.xml"

interface KernelSuPackageInfo {
  packageName: string
  versionName: string
  versionCode: number
  appLabel: string
  isSystem: boolean
  uid: number
}

function isBridgeAvailable() {
  const scope = globalThis as Record<string, unknown>
  const bridge = scope.ksu as Record<string, unknown> | undefined
  return typeof exec === "function" && typeof bridge?.exec === "function"
}

function safeListPackages(scope: string) {
  try {
    const packages = listPackages?.(scope)
    return Array.isArray(packages)
      ? packages.filter((value): value is string => typeof value === "string" && value.trim().length > 0)
      : []
  } catch {
    return []
  }
}

function safeGetPackagesInfo(packages: string[]) {
  try {
    if (!packages.length) {
      return []
    }

    const entries = getPackagesInfo?.(packages)
    return Array.isArray(entries)
      ? entries.filter(
          (value): value is KernelSuPackageInfo =>
            typeof value === "object" &&
            value !== null &&
            typeof value.packageName === "string",
        )
      : []
  } catch {
    return []
  }
}

function packageScore(info: KernelSuPackageInfo) {
  const name = info.packageName.toLowerCase()
  let score = 0

  if (name === "me.weishu.kernelsu") {
    score += 30
  }
  if (name.includes("kernelsu")) {
    score += 12
  }
  if (name.includes("ksunext")) {
    score += 10
  }
  if (typeof info.versionName === "string" && info.versionName.trim()) {
    score += 3
  }
  if (typeof info.versionCode === "number" && Number.isFinite(info.versionCode)) {
    score += 2
  }

  return score
}

function resolveKernelSuVersion() {
  const installedPackages = safeListPackages("all")
  const matchedPackages = installedPackages.filter((value) => {
    const name = value.toLowerCase()
    return name.includes("kernelsu") || name.includes("ksunext")
  })
  const candidates = [...new Set(matchedPackages)]
  const details = safeGetPackagesInfo(candidates).sort(
    (left, right) => packageScore(right) - packageScore(left),
  )
  const match = details[0]

  return {
    packageName: match?.packageName?.trim() || null,
    versionName: match?.versionName?.trim() || null,
    versionCode:
      typeof match?.versionCode === "number" && Number.isFinite(match.versionCode)
        ? match.versionCode
        : null,
  }
}

function safeModuleInfo(): Record<string, unknown> | null {
  try {
    const raw = moduleInfo?.()
    if (!raw) {
      return null
    }

    if (typeof raw === "string") {
      try {
        return JSON.parse(raw) as Record<string, unknown>
      } catch {
        return { raw }
      }
    }

    return raw as Record<string, unknown>
  } catch {
    return null
  }
}

function resolveModuleRoot() {
  const info = safeModuleInfo()
  const candidates = [
    info?.moduleDir,
    info?.modulePath,
    info?.path,
    typeof info?.moduleId === "string" ? `/data/adb/modules/${info.moduleId}` : undefined,
    typeof info?.id === "string" ? `/data/adb/modules/${info.id}` : undefined,
    MODULE_ROOT_FALLBACK,
  ].filter((value): value is string => Boolean(value))

  return candidates[0] ?? MODULE_ROOT_FALLBACK
}

function resolveDataRoot(moduleRoot: string) {
  const normalized = moduleRoot.trim()
  if (
    normalized.startsWith("/data/adb/modules/") ||
    normalized.startsWith("/data/adb/modules_update/")
  ) {
    return DATA_ROOT_FALLBACK
  }

  return normalized || DATA_ROOT_FALLBACK
}

function shellQuote(value: string) {
  return `'${value.replaceAll("'", `'\\''`)}'`
}

function stdinMarker() {
  return `__DUCK_TOOL_BOX_${Math.random().toString(36).slice(2).toUpperCase()}__`
}

function commandLine(args: string[], stdin?: string) {
  const root = resolveModuleRoot()
  const wrapper = shellQuote(`${root}/bin/duckctl.sh`)
  const joined = args.map(shellQuote).join(" ")

  if (!stdin) {
    return `${wrapper} ${joined}`
  }

  const marker = stdinMarker()
  return `cat <<'${marker}' | ${wrapper} ${joined}
${stdin}
${marker}`
}

function extractJson(stdout: string, stderr: string) {
  const lines = [stdout, stderr]
    .flatMap((value) => value.split(/\r?\n/))
    .map((value) => value.trim())
    .filter(Boolean)

  for (let index = lines.length - 1; index >= 0; index -= 1) {
    const line = lines[index]
    const start = line.indexOf("{")
    const end = line.lastIndexOf("}")
    if (start === -1 || end === -1 || end < start) {
      continue
    }

    const candidate = line.slice(start, end + 1)
    try {
      JSON.parse(candidate)
      return candidate
    } catch {
      // Fall through to the next candidate.
    }
  }

  const source = [stdout, stderr]
    .map((value) => value.trim())
    .filter(Boolean)
    .join("\n")
    .trim()
  const start = source.indexOf("{")
  const end = source.lastIndexOf("}")
  if (start === -1 || end === -1 || end < start) {
    throw new Error(source || "Duck ToolBox backend returned no JSON payload")
  }

  return source.slice(start, end + 1)
}

function nowIso() {
  return new Date().toISOString()
}

function execErrno(result: { errno?: number }) {
  return typeof result.errno === "number" ? result.errno : 0
}

function parsePatchLevelDay(value: string) {
  const digits = value.trim().replaceAll("-", "")
  if (digits.length < 8) {
    return 0
  }

  return Number(digits.slice(0, 8)) || 0
}

function parsePatchLevelMonth(value: string) {
  const digits = value.trim().replaceAll("-", "")
  if (digits.length < 6) {
    return 0
  }

  return Number(digits.slice(0, 6)) || 0
}

function hasPatchLevelDigits(value: number, digits: number) {
  return value > 0 && String(Math.trunc(value)).length === digits
}

function normalizeDetectedPatchLevels(device: DeviceInfo): DeviceInfo {
  const bootDay = hasPatchLevelDigits(device.boot_patch_level, 8)
    ? Math.trunc(device.boot_patch_level)
    : 0
  const vendorDay = hasPatchLevelDigits(device.vendor_patch_level, 8)
    ? Math.trunc(device.vendor_patch_level)
    : 0
  const systemMonth = hasPatchLevelDigits(device.system_patch_level, 6)
    ? Math.trunc(device.system_patch_level)
    : 0

  return {
    ...device,
    boot_patch_level: bootDay,
    system_patch_level: systemMonth,
    vendor_patch_level: vendorDay,
  }
}

function normalizeBootloaderState(
  vbmetaDeviceState: string,
  flashLocked: string,
) {
  if (vbmetaDeviceState.trim()) {
    return vbmetaDeviceState.trim()
  }

  if (flashLocked === "1") {
    return "locked"
  }

  if (flashLocked === "0") {
    return "unlocked"
  }

  return ""
}

function normalizeSecurityLevel(instances: string[]) {
  const available = instances.map((value) => value.trim().toLowerCase())
  if (available.includes("default")) {
    return "tee"
  }

  if (available.includes("strongbox")) {
    return "strongbox"
  }

  return "tee"
}

function parsePropMap(stdout: string) {
  const entries = stdout
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const [key, ...rest] = line.split("=")
      return [key, rest.join("=")] as const
    })

  return Object.fromEntries(entries)
}

async function remoteProvisioningInstances(root: string) {
  try {
    const result = await exec("cmd remote_provisioning list 2>/dev/null", {
      cwd: root,
      env: {
        DUCK_TOOLBOX_ROOT: root,
        DUCK_TOOLBOX_DATA_ROOT: resolveDataRoot(root),
      },
    })
    if (execErrno(result) !== 0) {
      return []
    }

    return result.stdout
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
  } catch {
    return []
  }
}

function unavailableEnvelope<T>(command: string): Envelope<T> {
  return {
    ok: false,
    command,
    data: null,
    error: {
      code: "ksu_unavailable",
      message: translate("messages.ksuUnavailable"),
    },
    ts: Math.floor(Date.now() / 1000),
  }
}

export function bridgeStatus(): BridgeStatus {
  const available = isBridgeAvailable()
  const moduleRoot = resolveModuleRoot()
  const version = available ? resolveKernelSuVersion() : {
    packageName: null,
    versionName: null,
    versionCode: null,
  }

  return {
    mode: available ? "kernelsu" : "unavailable",
    moduleRoot,
    dataRoot: resolveDataRoot(moduleRoot),
    packageName: version.packageName,
    versionName: version.versionName,
    versionCode: version.versionCode,
  }
}

export function pushToast(message: string) {
  try {
    toast?.(message)
  } catch {
    console.info(message)
  }
}

export function historyEntry<T>(envelope: Envelope<T>): CommandHistoryEntry {
  return {
    command: envelope.command,
    ok: envelope.ok,
    at: nowIso(),
    message: envelope.ok
      ? translate("messages.commandCompleted")
      : envelope.error?.message ?? translate("messages.commandFailed"),
  }
}

export async function systemProfileDefaults(): Promise<ProfileData | null> {
  if (!isBridgeAvailable()) {
    return null
  }

  const keys = [
    "ro.product.brand",
    "ro.product.model",
    "ro.product.device",
    "ro.product.name",
    "ro.product.manufacturer",
    "ro.build.fingerprint",
    "ro.build.version.release",
    "remote_provisioning.hostname",
    "ro.boot.verifiedbootstate",
    "ro.boot.vbmeta.device_state",
    "ro.boot.vbmeta.digest",
    "ro.boot.flash.locked",
    "ro.build.version.security_patch",
    "ro.system.build.version.security_patch",
    "ro.bootimage.build.version.security_patch",
    "ro.vendor.build.security_patch",
  ]

  const command = keys
    .map((key) => `printf '%s=' ${shellQuote(key)}; getprop ${shellQuote(key)}`)
    .join("; ")

  try {
    const root = resolveModuleRoot()
    const dataRoot = resolveDataRoot(root)
    const [result, instances] = await Promise.all([
      exec(command, {
        cwd: root,
        env: {
          DUCK_TOOLBOX_ROOT: root,
          DUCK_TOOLBOX_DATA_ROOT: dataRoot,
        },
      }),
      remoteProvisioningInstances(root),
    ])
    if (execErrno(result) !== 0) {
      return null
    }

    const props = parsePropMap(result.stdout)
    const serverHost = props["remote_provisioning.hostname"]
      ?.trim()
      .replace(/^https?:\/\//, "")
      .replace(/\/v1\/?$/, "")
      .replace(/\/+$/, "")
    const device = normalizeDetectedPatchLevels({
      brand: props["ro.product.brand"] ?? "",
      model: props["ro.product.model"] ?? "",
      device: props["ro.product.device"] ?? "",
      product: props["ro.product.name"] ?? "",
      manufacturer: props["ro.product.manufacturer"] ?? "",
      fused: 1,
      vb_state: props["ro.boot.verifiedbootstate"] ?? "",
      os_version: props["ro.build.version.release"] ?? "",
      security_level: normalizeSecurityLevel(instances),
      bootloader_state: normalizeBootloaderState(
        props["ro.boot.vbmeta.device_state"] ?? "",
        props["ro.boot.flash.locked"] ?? "",
      ),
      boot_patch_level: parsePatchLevelDay(
        props["ro.bootimage.build.version.security_patch"] ??
          props["ro.build.version.security_patch"] ??
          "",
      ),
      system_patch_level: parsePatchLevelMonth(
        props["ro.system.build.version.security_patch"] ??
          props["ro.build.version.security_patch"] ??
          "",
      ),
      vendor_patch_level: parsePatchLevelDay(
        props["ro.vendor.build.security_patch"] ??
          props["ro.build.version.security_patch"] ??
          "",
      ),
      vbmeta_digest: props["ro.boot.vbmeta.digest"] ?? "",
      dice_issuer: "Android",
      dice_subject: "KeyMint",
    })

    return {
      key_source: { kind: "unset" },
      device,
      fingerprint: {
        value: props["ro.build.fingerprint"] ?? "",
      },
      server_url: serverHost
        ? `https://${serverHost}/v1`
        : defaultProfile().server_url,
      num_keys: defaultProfile().num_keys,
      output_path: defaultProfile().output_path,
    }
  } catch {
    return null
  }
}

function failureEnvelope<T>(
  command: string,
  code: string,
  error: unknown,
  details?: Record<string, unknown>,
): Envelope<T> {
  return {
    ok: false,
    command,
    data: null,
    error: {
      code,
      message:
        error instanceof Error && error.message.trim()
          ? error.message
          : translate("messages.unexpectedError"),
      details,
    },
    ts: Math.floor(Date.now() / 1000),
  }
}

async function execJson<T>(args: string[], payload?: unknown): Promise<Envelope<T>> {
  const command = args.join(".")
  if (!isBridgeAvailable()) {
    return unavailableEnvelope<T>(command)
  }

  const root = resolveModuleRoot()
  const dataRoot = resolveDataRoot(root)
  const stdin = payload ? JSON.stringify(payload, null, 2) : undefined
  const shell = commandLine(args, stdin)
  let result: Awaited<ReturnType<typeof exec>>

  try {
    result = await exec(shell, {
      cwd: root,
      env: {
        DUCK_TOOLBOX_ROOT: root,
        DUCK_TOOLBOX_DATA_ROOT: dataRoot,
      },
    })
  } catch (error) {
    return failureEnvelope<T>(command, "exec_failed", error, {
      shell,
      root,
    })
  }

  try {
    return JSON.parse(extractJson(result.stdout, result.stderr)) as Envelope<T>
  } catch (error) {
    return failureEnvelope<T>(command, "json_parse_error", error, {
      errno: execErrno(result),
      stdout: result.stdout,
      stderr: result.stderr,
    })
  }
}

export function profileShow() {
  return execJson<ProfileEnvelopeData>(["rkp", "profile", "show", "--json"])
}

export function profileSave(profile: ProfileData) {
  return execJson<ProfileEnvelopeData>(
    ["rkp", "profile", "save", "--stdin-json", "--json"],
    profile,
  )
}

export function profileClear() {
  return execJson<{ cleared: boolean; paths: PathsInfo }>([
    "rkp",
    "profile",
    "clear",
    "--json",
  ])
}

export function infoCommand() {
  return execJson<InfoData>(["rkp", "info", "--json"])
}

export function deviceIdsDefaultsCommand() {
  return execJson<DeviceIdsProfileData>(["device-ids", "defaults", "--json"])
}

export function deviceIdsProvisionCommand(profile: DeviceIdsProfileData) {
  return execJson<DeviceIdsProvisionData>(
    ["device-ids", "provision", "--stdin-json", "--json"],
    profile,
  )
}

export function provisionCommand() {
  return execJson<ProvisionData>(["rkp", "provision", "--json"])
}

export function keyboxCommand() {
  return execJson<KeyboxData>(["rkp", "keybox", "--json"])
}

export async function replaceTrickyStoreKeyboxCommand(sourcePath: string) {
  const command = "tricky_store.keybox.replace"
  if (!isBridgeAvailable()) {
    return unavailableEnvelope<TrickyStoreKeyboxInstallData>(command)
  }

  const root = resolveModuleRoot()
  const dataRoot = resolveDataRoot(root)
  const source = sourcePath.trim()
  const target = TRICKY_STORE_KEYBOX_PATH
  const shell = `
set -eu
source_path=${shellQuote(source)}
target_path=${shellQuote(target)}
timestamp="$(date +%Y%m%d-%H%M%S)"
backup_path=""

if [ ! -f "$source_path" ]; then
  echo "Generated keybox file not found: $source_path" >&2
  exit 1
fi

mkdir -p /data/adb/tricky_store

if [ -e "$target_path" ]; then
  backup_path="$target_path.$timestamp.bak"
  mv "$target_path" "$backup_path"
fi

cp "$source_path" "$target_path"
chmod 0600 "$target_path" 2>/dev/null || true
chown 0:0 "$target_path" 2>/dev/null || true

printf '%s\\n' "$backup_path"
`.trim()

  let result: Awaited<ReturnType<typeof exec>>
  try {
    result = await exec(shell, {
      cwd: root,
      env: {
        DUCK_TOOLBOX_ROOT: root,
        DUCK_TOOLBOX_DATA_ROOT: dataRoot,
      },
    })
  } catch (error) {
    return failureEnvelope<TrickyStoreKeyboxInstallData>(
      command,
      "exec_failed",
      error,
      {
        root,
        source_path: source,
        target_path: target,
      },
    )
  }

  if (execErrno(result) !== 0) {
    return failureEnvelope<TrickyStoreKeyboxInstallData>(
      command,
      "tricky_store_replace_failed",
      new Error(result.stderr.trim() || result.stdout.trim() || "replace failed"),
      {
        errno: execErrno(result),
        stdout: result.stdout,
        stderr: result.stderr,
        source_path: source,
        target_path: target,
      },
    )
  }

  return {
    ok: true,
    command,
    data: {
      source_path: source,
      target_path: target,
      backup_path: result.stdout.trim() || null,
    },
    error: null,
    ts: Math.floor(Date.now() / 1000),
  } satisfies Envelope<TrickyStoreKeyboxInstallData>
}

export function verifyCommand(path: string) {
  return execJson<VerifyData>(["rkp", "verify", path.trim(), "--json"])
}

export function artifactsCommand() {
  return execJson<ArtifactsData>(["artifacts", "list", "--json"])
}
