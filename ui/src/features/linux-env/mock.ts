import { MockFailure } from "@/core/mock"
import type { MockModule } from "@/core/mock"
import { configSchema } from "./api"
import type { EnvironmentView } from "./api"
const scenario = new URLSearchParams(location.search).get("linuxEnv")
let serial = 1
const digest = () => (serial++).toString(16).padStart(64, "0")
const make = (id: string, name: string): EnvironmentView => ({
  environment: {
    id,
    instance: digest(),
    archive_sha256: "ab".repeat(32),
    expanded_bytes: 1073741824,
    entries: 12000,
    created_at: 1,
    config: { name, auto_start: false, dns: [], services: [] },
  },
  revision: digest(),
  phase: "stopped",
  message: "",
  runtime: null,
})
let environments = scenario === "empty" ? [] : [make("debian", "Debian")]
let job: {
  token: string
  phase: string
  entries: number
  bytes: number
  code: string
  message: string
} | null = null
let imported: EnvironmentView | null = null
let ticks = 0
let statusCalls = 0
function target(input: unknown) {
  const request = input as { id: string; revision: string; confirmed: boolean }
  if (!request.confirmed) throw new MockFailure("confirmation_required", "Confirm first")
  const view = environments.find((v) => v.environment.id === request.id)
  if (!view || view.revision !== request.revision || scenario === "changed")
    throw new MockFailure("state_changed", "Environment changed")
  return view
}
const mock: MockModule = {
  feature: { id: "linux-env", contract: 1 },
  handlers: {
    "linux-env.status": () => {
      statusCalls++
      if (scenario === "config-change" && environments[0]) {
        if (statusCalls === 2) {
          environments[0].environment.config.name = "Saved elsewhere"
          environments[0].revision = digest()
        }
      }
      if (scenario === "exited" && environments[0])
        environments[0].phase = statusCalls >= 2 ? "unknown" : "running"
      if (scenario === "cancelled" && !job)
        job = {
          token: digest(),
          phase: "failed",
          entries: 0,
          bytes: 0,
          code: "cancelled",
          message: "Import cancelled",
        }
      if (job && !["done", "failed"].includes(job.phase) && ++ticks >= 2) {
        job = { ...job, phase: "done", entries: 12000, bytes: 1073741824 }
        if (imported) environments.push(imported)
        imported = null
      }
      return {
        supported: scenario !== "readonly",
        runtime_ready: scenario !== "missing",
        busy: !!job && !["done", "failed"].includes(job.phase),
        environments,
        job,
      }
    },
    "linux-env.inspect": (args) => {
      if (args[0]?.includes("missing")) throw new MockFailure("invalid_archive", "No such archive")
      return { path: args[0], bytes: 33554432, sha256: "ab".repeat(32) }
    },
    "linux-env.import": (_args, _flags, input) => {
      const request = input as { id: string; config: unknown; confirmed: boolean }
      if (!request.confirmed) throw new MockFailure("confirmation_required", "Confirm first")
      if (environments.some((v) => v.environment.id === request.id))
        throw new MockFailure("invalid_input", "ID already exists")
      const config = configSchema.parse(request.config)
      imported = make(request.id, config.name)
      imported.environment.config = config
      ticks = 0
      job = { token: digest(), phase: "copying", entries: 0, bytes: 0, code: "", message: "" }
      return job
    },
    "linux-env.cancel": (_args, flags) => {
      if (!job || job.token !== flags.token)
        throw new MockFailure("state_changed", "Import changed")
      job = { ...job, phase: "failed", code: "cancelled", message: "Import cancelled" }
      imported = null
      return true
    },
    "linux-env.start": (_args, _flags, input) => {
      const view = target(input)
      view.phase = "running"
      view.runtime = { config: structuredClone(view.environment.config) }
      return true
    },
    "linux-env.stop": (_args, _flags, input) => {
      const view = target(input)
      if (scenario === "unknown")
        throw new MockFailure("operation_unknown", "Unable to confirm termination")
      view.phase = "stopped"
      return true
    },
    "linux-env.remove": (_args, _flags, input) => {
      const view = target(input)
      if (view.phase !== "stopped") throw new MockFailure("operation_busy", "Stop first")
      environments = environments.filter((v) => v !== view)
      return true
    },
    "linux-env.save": (_args, _flags, input) => {
      const request = input as { target: unknown; config: unknown }
      const view = target(request.target)
      view.environment.config = configSchema.parse(request.config)
      view.revision = digest()
      return view.environment
    },
    "linux-env.logs": () =>
      scenario === "empty-logs"
        ? ""
        : "stdout.log\n[web] starting\nListening on 8080\nstderr.log\n",
  },
}
export default mock
