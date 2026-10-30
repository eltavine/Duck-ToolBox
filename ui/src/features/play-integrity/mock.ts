import type { MockModule } from "@/core/mock"
import { MockFailure } from "@/core/mock"
import type { Options, Preview } from "./api"
import type { ArchiveCatalog, ArchiveChannel } from "./pixel-archive-api"
import { identityPairs } from "./fields"
import type { libraryApi } from "./library-api"

let content =
  "MODEL=Pixel 9 Pro\nFINGERPRINT=google/caiman/caiman:16/ID/123:user/release-keys\nSECURITY_PATCH=2026-10-05\nspoofProvider=0\n"
let revision = "initial"
const revisions = new Map([[content, revision]])
function contentRevision() {
  const next = revisions.get(content) ?? `configuration-${revisions.size}`
  revisions.set(content, next)
  return next
}
let previous = ""
let count = 0
let cancelled = false
let stopRevision = "initial"
let processRestart = false
let rebootRequired = false
let interrupted = new URLSearchParams(location.search).get("interrupted-transaction") === "1"
const profiles: Awaited<ReturnType<typeof libraryApi.profiles>>["profiles"] = []
const pixelDevices = [
  { id: "cubs", name: "Pixel 11", api: 37 },
  { id: "yogi", name: "Pixel 11 Pro Fold", api: 37 },
  { id: "stallion", name: "Pixel 10a", api: 36 },
  { id: "rango", name: "Pixel 10 Pro Fold", api: 36 },
  { id: "felix", name: "Pixel Fold", api: 33 },
  { id: "tangorpro", name: "Pixel Tablet", api: 33 },
  { id: "bluejay", name: "Pixel 6a", api: 32 },
  { id: "lynx", name: "Pixel 7a", api: 33 },
  { id: "sailfish", name: "Pixel", api: 25 },
]
const profileContents = new Map<string, string>()
const invalidProfiles = new URLSearchParams(location.search).has("corrupt-pif-library")
  ? [{ id: "a".repeat(64), error: "Invalid snapshot fixture" }]
  : []
let corruptHistory = new URLSearchParams(location.search).has("corrupt-pif-history")
const records: Awaited<ReturnType<typeof libraryApi.records>>["records"] = []
const options: Options = {
  enabled: true,
  mode: "full",
  skipdelprop: false,
  skippersistprop: false,
  skipmatchdevice: false,
}
const archive: ArchiveCatalog = {
  source: "https://github.com/VictorGabriel7Dev/build.prop",
  source_revision: "a".repeat(40),
  fetched: Math.floor(Date.now() / 1000),
  generated: "2026-10-30T00:00:00Z",
  devices: Object.fromEntries(
    ["bluejay", "lynx"].map((id) => [
      id,
      {
        model: id === "bluejay" ? "Pixel 6a" : "Pixel 7a",
        first_api_level: id === "bluejay" ? 32 : 33,
        channels: Object.fromEntries(
          (["release", "beta", "canary"] as const)
            .filter((channel) => id !== "lynx" || channel === "release")
            .map((channel) => [
              channel,
              {
                latest: `${id}/${id}${channel === "release" ? "" : `_${channel}`}_5678.prop`,
                build:
                  channel === "canary" ? "CANARY.1" : channel === "beta" ? "BETA.1" : "ARCHIVE.1",
                incremental: "5678",
                security_patch: "2026-10-05",
                android: "17",
                vbmeta_digest: "b".repeat(64),
              },
            ]),
        ),
      },
    ]),
  ),
}
const entries = (value: string) =>
  Object.fromEntries(
    value
      .split("\n")
      .filter((line) => line.includes("=") && !line.startsWith("#"))
      .map((line) => {
        const at = line.indexOf("=")
        return [line.slice(0, at), line.slice(at + 1)]
      }),
  )
const config = () => ({
  content,
  json: JSON.stringify(entries(content)),
  entries: entries(content),
  revision,
})
function preview(value: string) {
  const next = value.trim().startsWith("{")
    ? (JSON.parse(value) as Record<string, string>)
    : entries(value)
  const before = entries(content)
  return {
    content: Object.values(next).some((value) => value.includes("#"))
      ? JSON.stringify(next, null, 2)
      : Object.entries(next)
          .map(([key, value]) => `${key}=${value}\n`)
          .join(""),
    json: JSON.stringify(next),
    entries: next,
    revision,
    changes: [...new Set([...Object.keys(before), ...Object.keys(next)])]
      .filter((key) => before[key] !== next[key])
      .map((key) => ({ key, before: before[key] ?? null, after: next[key] ?? null })),
    suggested_patch: next.SECURITY_PATCH ?? null,
  }
}
const mock: MockModule = {
  feature: { id: "play-integrity", contract: 1 },
  handlers: {
    "play-integrity.pixel-archive-catalog": () => {
      if (new URLSearchParams(location.search).has("archive-offline"))
        throw new MockFailure("engine_error", "Archive unavailable")
      return archive
    },
    "play-integrity.pixel-archive-preview": (_a, _f, input) => {
      const value = input as {
        device: string
        channel: ArchiveChannel
        source_revision: string
        build: string
        incremental: string
      }
      const device = archive.devices[value.device]
      const build = device?.channels[value.channel]
      if (
        !build ||
        value.source_revision !== archive.source_revision ||
        value.build !== build.build ||
        value.incremental !== build.incremental
      )
        throw new MockFailure("engine_error", "Archive selection changed")
      const product = value.channel === "release" ? value.device : `${value.device}_beta`
      const fingerprint = `google/${product}/${value.device}:${value.channel === "canary" ? "CANARY" : "17"}/${build.build}/${build.incremental}:user/release-keys`
      return {
        ...preview(
          `MODEL=${device!.model}\nDEVICE=${value.device}\nPRODUCT=${product}\nID=${build.build}\nINCREMENTAL=${build.incremental}\nFINGERPRINT=${fingerprint}\nSECURITY_PATCH=${build.security_patch}\nDEVICE_INITIAL_SDK_INT=${device!.first_api_level}\nspoofProvider=0\n`,
        ),
        provenance: {
          source: archive.source,
          source_revision: archive.source_revision,
          device: value.device,
          build: build.build,
          fetched: archive.fetched,
          patch_known: true,
          channel: value.channel,
          incremental: build.incremental,
          security_patch: build.security_patch,
          vbmeta_digest: build.vbmeta_digest,
          file_sha256: "c".repeat(64),
          fingerprint,
          model: device!.model,
          first_api_level: device!.first_api_level,
        },
      }
    },
    "play-integrity.pixel-devices": () => ({
      default: "bluejay",
      devices: pixelDevices,
    }),
    "play-integrity.pixel-builds": (_a, _f, input) => {
      const device = (input as { device: string }).device
      const model = pixelDevices.find((item) => item.id === device)!
      const release = model.api === 37 ? "17" : "16"
      return {
        device,
        model: model.name,
        source: "https://flash.android.com/",
        fetched: Math.floor(Date.now() / 1000),
        builds: ["1234", "1233"].map((incremental) => ({
          id: "RELEASE.1",
          incremental,
          release,
          notes: "",
          fingerprint: `google/${device}/${device}:${release}/RELEASE.1/${incremental}:user/release-keys`,
        })),
      }
    },
    "play-integrity.pixel-preview": (_a, _f, input) => {
      const device = (input as { device: string }).device
      const model = pixelDevices.find((item) => item.id === device)!
      const release = model.api === 37 ? "17" : "16"
      const incremental = (input as { incremental?: string }).incremental ?? "1234"
      return {
        ...preview(
          `MODEL=${model.name}\nDEVICE=${device}\nDEVICE_INITIAL_SDK_INT=${model.api}\nID=RELEASE.1\nFINGERPRINT=google/${device}/${device}:${release}/RELEASE.1/${incremental}:user/release-keys\n`,
        ),
        provenance: {
          source: "https://flash.android.com/",
          device,
          build: "RELEASE.1",
          fetched: Math.floor(Date.now() / 1000),
          patch_known: false,
        },
        warnings: [{ code: "pixel_patch_unknown", fields: [] }],
      }
    },
    "play-integrity.record-import": (args, flags, input) => {
      const value = input as {
        revision: string
        report: {
          requestDetails: { requestPackageName: string }
          deviceIntegrity: { deviceRecognitionVerdict?: string[] }
        }
      }
      const labels = value.report.deviceIntegrity.deviceRecognitionVerdict ?? []
      return mock.handlers["play-integrity.record-save"]!(args, flags, {
        app: value.report.requestDetails.requestPackageName,
        version: "",
        note: "",
        revision: value.revision,
        basic: labels.includes("MEETS_BASIC_INTEGRITY") ? "pass" : "omitted",
        device: labels.includes("MEETS_DEVICE_INTEGRITY") ? "pass" : "omitted",
        strong: labels.includes("MEETS_STRONG_INTEGRITY") ? "pass" : "omitted",
      })
    },
    "play-integrity.profile-list": () => ({
      profiles: [...profiles],
      invalid_profiles: [...invalidProfiles],
    }),
    "play-integrity.profile-save": (_args, _flags, input) => {
      const value = input as { name: string; content: string; provenance?: Preview["provenance"] }
      const id = String(Date.now())
      profiles.unshift({
        id,
        name: value.name,
        created: Date.now() / 1000,
        revision,
        provenance: value.provenance,
      })
      profileContents.set(id, value.content)
      return { profiles: [...profiles], invalid_profiles: [...invalidProfiles] }
    },
    "play-integrity.profile-preview": (_args, _flags, input) => {
      const id = (input as { id: string }).id
      return {
        ...preview(profileContents.get(id) ?? ""),
        provenance: profiles.find((item) => item.id === id)?.provenance,
      }
    },
    "play-integrity.profile-delete": (_args, _flags, input) => {
      const id = (input as { id: string }).id
      const index = profiles.findIndex((p) => p.id === id)
      if (index >= 0) profiles.splice(index, 1)
      const invalid = invalidProfiles.findIndex((p) => p.id === id)
      if (invalid >= 0) invalidProfiles.splice(invalid, 1)
      profileContents.delete(id)
      return { profiles: [...profiles], invalid_profiles: [...invalidProfiles] }
    },
    "play-integrity.inspect": () => ({
      revision,
      read_error: null,
      runtime_observed: false,
      processes: [],
      runtime_error: null,
      verdict_verified: false,
      rows: identityPairs.map(([field, property]) => ({
        field,
        property,
        system: null,
        saved_build: entries(content)[field] ?? null,
        saved_property: entries(content)[property] ?? null,
        observed: null,
      })),
    }),
    "play-integrity.record-list": () => {
      if (corruptHistory) throw new MockFailure("engine_error", "Invalid test history fixture")
      return {
        records: [...records],
        user_reported: true,
        verdict_verified: false,
      }
    },
    "play-integrity.record-save": (_args, _flags, input) => {
      const v = input as Parameters<typeof libraryApi.saveRecord>[0]
      if (v.revision !== revision)
        throw new MockFailure(
          "engine_error",
          "Configuration changed; refresh before recording a test",
        )
      records.unshift({
        id: String(Date.now()),
        created: Date.now() / 1000,
        app: v.app,
        version: v.version,
        note: v.note,
        results: [v.basic, v.device, v.strong],
        revision,
        model: entries(content).MODEL ?? "",
        fingerprint: entries(content).FINGERPRINT ?? "",
        enabled: options.enabled,
        mode: options.mode,
        installation_source: v.installation_source ?? "",
      })
      records.splice(24)
      return { records: [...records], user_reported: true, verdict_verified: false }
    },
    "play-integrity.record-clear": () => {
      corruptHistory = false
      records.splice(0)
      return { records: [], user_reported: true, verdict_verified: false }
    },
    "play-integrity.power": () => ({
      enabled: options.enabled,
      disabled_by_marker: !options.enabled,
      stop_revision: stopRevision,
      cleanup_pending: false,
      issues: [],
    }),
    "play-integrity.disable": () => {
      options.enabled = false
      cancelled = true
      stopRevision = String(Date.now())
      return {
        enabled: false,
        disabled_by_marker: true,
        stop_revision: stopRevision,
        cleanup_pending: false,
        issues: [],
        reboot_required: true,
        process_restart_required: true,
      }
    },
    "integrity-transaction.status": () => ({
      pending: interrupted,
      journal_path: "/data/adb/duck-toolbox/var/integrity-transaction/journal.json",
      journal: null,
    }),
    "integrity-transaction.recover": () => {
      interrupted = false
      return { restored: true, restart_required: true }
    },
    "integrity-transaction.apply": (args, flags, input) => {
      if (interrupted)
        throw new MockFailure("internal_error", "Interrupted transaction requires recovery")
      const request = input as { engine: unknown; patch?: unknown }
      const result = mock.handlers["play-integrity.apply"]!(args, flags, request.engine)
      if (request.patch) rebootRequired = true
      if (request.patch && new URLSearchParams(location.search).get("patch-fail") === "1") {
        mock.handlers["play-integrity.restore"]!(args, flags, {})
        throw new MockFailure("backend_rule", "Patch fixture rejected")
      }
      return result
    },
    "play-integrity.status": () => {
      if (new URLSearchParams(location.search).has("invalid-pif"))
        throw new MockFailure("engine_error", "Invalid configuration fixture")
      return {
        upstream_revision: "23ec56c2440c4fd1a5e893a23ef7861d6366a828",
        options: { ...options },
        has_config: true,
        zygisk_detected: true,
        conflicts: [],
        reboot_required: rebootRequired,
        process_restart_required: processRestart,
        state: options.enabled
          ? options.mode === "scripts-only"
            ? "scripts-only"
            : "configured"
          : "disabled",
        data_dir:
          new URLSearchParams(location.search).get("pif-data-dir") ?? "/data/adb/play-integrity",
        verdict_verified: false,
        app_replace_list: "# ROM conflict paths\n",
      }
    },
    "play-integrity.config": config,
    "play-integrity.preview": (_args, _flags, input) =>
      preview(String((input as { content: string }).content)),
    "play-integrity.import-preview": () =>
      preview("MODEL=Imported\nSECURITY_PATCH=2026-10-05\nEXTRA=preserved\n"),
    "play-integrity.apply": (_args, _flags, input) => {
      const request = input as { content: string; revision: string }
      if (request.revision !== revision)
        throw new MockFailure("engine_error", "Configuration changed; preview again")
      previous = content
      content = preview(request.content).content
      revision = contentRevision()
      processRestart = true
      return { config: config(), backup: "mock-backup", restart_required: true }
    },
    "play-integrity.restore": () => {
      content = previous
      revision = contentRevision()
      processRestart = true
      return { config: config(), restart_required: true }
    },
    "play-integrity.options": (_args, _flags, input) => {
      Object.assign(options, input)
      return { options: { ...options }, reboot_required: true }
    },
    "play-integrity.task-start": () => {
      count = 0
      cancelled = false
      return { id: "fixture-task", state: "running", result: null, error: null, output: "" }
    },
    "play-integrity.task-status": () => ({
      id: "fixture-task",
      state: cancelled ? "cancelled" : ++count < 2 ? "running" : "completed",
      result:
        cancelled || count < 2
          ? null
          : {
              ...preview("MODEL=Pixel Canary\nSECURITY_PATCH=2026-10-05\nspoofProvider=0\n"),
              metadata: { released: "2026-10-01", estimated_expiry: "2026-11-12" },
            },
      error: null,
      output: "Pixel Canary metadata fetched over HTTPS",
    }),
    "play-integrity.task-cancel": () => {
      cancelled = true
      return { id: "fixture-task", state: "cancelled", result: null, error: null, output: "" }
    },
    "play-integrity.restart": () => {
      processRestart = false
      return {
        processes: [
          { process: "com.google.android.gms.unstable", exit_code: 0, output: "" },
          { process: "com.android.vending", exit_code: 0, output: "" },
        ],
        verdict_verified: false,
      }
    },
    "play-integrity.diagnose": () => ({
      log: "PIF/Native: Injection initialized for com.google.android.gms.unstable",
      verdict_verified: false,
    }),
  },
}
export default mock
