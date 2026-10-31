// @vitest-environment node
import { afterEach, describe, expect, it } from "vitest"
import { mkdirSync, writeFileSync } from "node:fs"
import { bridge, setBridge } from "@/core/bridge"
import { CONTRACT, hasDuckd, installHost, type Host } from "@/core/testing/host"
import path from "node:path"
import { languageApi } from "./api"

let host: Host | undefined
afterEach(() => {
  setBridge(null)
  host?.dispose()
  host = undefined
})

describe.skipIf(!hasDuckd)("misc language API contract", CONTRACT, () => {
  it("saves isolated user rules, enables explicitly and rejects stale activation after stop", async () => {
    host = installHost()
    const module = bridge().hostModule()!.dir
    mkdirSync(path.join(module, "play-integrity"), { recursive: true })
    writeFileSync(path.join(module, "play-integrity/language-spoof.version"), "1\n")
    writeFileSync(path.join(module, "play-integrity/classes.dex"), "dex fixture")
    mkdirSync(path.join(module, "zygisk"), { recursive: true })
    writeFileSync(path.join(module, "zygisk/arm64-v8a.so"), "ELF fixture")
    const initial = await languageApi.status()
    expect(initial.enabled).toBe(false)
    const saved = await languageApi.save(initial.revision, [
      { package_name: "com.example.app", user_id: 0, locale: "en-us" },
      { package_name: "com.example.app", user_id: 10, locale: "ja-jp" },
    ])
    expect(saved.config?.rules.map((rule) => rule.locale)).toEqual(["en-US", "ja-JP"])
    expect(saved.enabled).toBe(false)
    await languageApi.disable()
    await expect(languageApi.enable(saved.revision)).rejects.toMatchObject({
      code: "stale_revision",
    })
    const stopped = await languageApi.status()
    expect((await languageApi.enable(stopped.revision)).enabled).toBe(true)
    await languageApi.disable()
    expect((await languageApi.status()).config?.rules).toHaveLength(2)
  })

  it("retains corrupt rules while allowing an unconditional stop", async () => {
    host = installHost({})
    const saved = await languageApi.save((await languageApi.status()).revision, [
      { package_name: "com.example.app", user_id: 0, locale: "en" },
    ])
    expect(saved.config_error).toBeNull()
    writeFileSync(path.join(bridge().hostModule()!.dir, "var/misc/language/rules.json"), "corrupt")
    expect((await languageApi.status()).config_error).toBe("invalid_config")
    expect((await languageApi.disable()).enabled).toBe(false)
    await expect(languageApi.save(saved.revision, [])).rejects.toMatchObject({
      code: "stale_revision",
    })
  })
})
