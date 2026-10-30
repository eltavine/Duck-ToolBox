import type { MockModule } from "@/core/mock"
import { MockFailure } from "@/core/mock"

let content = "debug = false\n\n[templates]\n"
let revision = "initial"
let backup = content
let backupAfter = ""
let enabled = false
let stopRevision = "initial"
const summary = (value: string) => ({
  templates: [...value.matchAll(/^\[templates\.([^\]]+)\]/gm)].map((m) => m[1]!),
  targets: [...new Set([...value.matchAll(/"(com\.[\w.@]+)"/g)].map((m) => m[1]!))],
  warnings: [],
  global_effects: /companion_resetprop\s*=\s*true|dpi\s*=/.test(value),
})
const config = () => ({ content, revision, summary: summary(content), error: null })
function input(value: unknown) {
  return value as Record<string, string | boolean>
}
function check(value: unknown) {
  if (input(value).revision !== revision)
    throw new MockFailure("engine_error", "Configuration changed")
}
const mock: MockModule = {
  feature: { id: "device-faker", contract: 2 },
  handlers: {
    "device-faker.status": () => ({
      enabled,
      stop_revision: stopRevision,
      parent_active: true,
      startup_ready: !new URLSearchParams(location.search).has("faker-startup-pending"),
      startup_failed: new URLSearchParams(location.search).has("faker-startup-failed"),
      cleanup_pending: new URLSearchParams(location.search).has("faker-cleanup-pending"),
      payload_installed: true,
      payload_active: true,
      payload_pending: false,
      conflict: false,
      injection_verified: false,
      legacy_available: true,
      config: config(),
      upstream_revision: "71e778404a8b1ded3271aa09f936f688141d6b31",
    }),
    "device-faker.config": config,
    "device-faker.preview": (_a, _f, value) => {
      const draft = String(input(value).content)
      if (draft.includes("invalid["))
        throw new MockFailure("engine_error", "Invalid TOML", { code: "invalid_config" })
      return { content: draft, revision, summary: summary(draft) }
    },
    "device-faker.import-preview": () => {
      const draft = '[[apps]]\npackage = "com.example.legacy@999"\nmodel = "Legacy"\n'
      return { content: draft, revision, summary: summary(draft) }
    },
    "device-faker.apply": (_a, _f, value) => {
      check(value)
      backup = content
      content = String(input(value).content)
      backupAfter = revision = `${Date.now()}`
      return config()
    },
    "device-faker.restore-preview": () => {
      if (backupAfter !== revision) throw new MockFailure("engine_error", "Backup is stale")
      return { content: backup, revision, summary: summary(backup) }
    },
    "device-faker.restore": (_a, _f, value) => {
      check(value)
      if (backupAfter !== revision) throw new MockFailure("engine_error", "Backup is stale")
      content = backup
      revision = `${Date.now()}`
      return config()
    },
    "device-faker.enable": (_a, _f, value) => {
      check(value)
      if (input(value).stop_revision !== stopRevision || !summary(content).targets.length) {
        throw new MockFailure("engine_error", "Refresh or configure targets first")
      }
      enabled = true
      return { enabled, stop_revision: stopRevision }
    },
    "device-faker.disable": () => {
      enabled = false
      stopRevision = `${Date.now()}`
      return { enabled, stop_revision: stopRevision }
    },
  },
}
export default mock
