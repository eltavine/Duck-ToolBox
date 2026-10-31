import { MockFailure, type MockModule } from "@/core/mock"
import type { LanguageRule, LanguageStatus } from "./language/api"

const scenario = new URLSearchParams(location.search).get("language")
let sequence = 0
let enabled = false
let rules: LanguageRule[] =
  scenario === "rules" ? [{ package_name: "com.tencent.mm", user_id: 0, locale: "en-US" }] : []
const revision = () => `language-${sequence}`
const status = (): LanguageStatus => ({
  config: scenario === "corrupt" ? null : { rules: structuredClone(rules) },
  config_error: scenario === "corrupt" ? "invalid_config" : null,
  revision: revision(),
  enabled,
  payload_installed: scenario !== "unavailable",
  parent_active: true,
  restart_required: true,
})
function request(input: unknown) {
  const value = input as { revision: string; config: { rules: LanguageRule[] } }
  if (value.revision !== revision())
    throw new MockFailure("stale_revision", "Refresh before saving")
  return value
}
const mock: MockModule = {
  feature: { id: "misc", contract: 1 },
  handlers: {
    "misc.language.status": status,
    "misc.language.save": (_args, _flags, input) => {
      rules = structuredClone(request(input).config.rules)
      sequence += 1
      return status()
    },
    "misc.language.enable": (_args, _flags, input) => {
      request(input)
      if (scenario === "corrupt") throw new MockFailure("invalid_config", "Invalid configuration")
      if (!rules.length) throw new MockFailure("empty_rules", "Add an application first")
      enabled = true
      sequence += 1
      return status()
    },
    "misc.language.disable": () => {
      enabled = false
      sequence += 1
      return { enabled: false, restart_required: true }
    },
  },
}
export default mock
