// @vitest-environment node
import { afterEach, describe, expect, it } from "vitest"

import { featureManifest, listFiles } from "@/core/duckd"
import type { Host, Server } from "@/core/testing/host"
import { DEVICE_PROPS, CONTRACT, hasDuckd, installHost, serve } from "@/core/testing/host"

import { systemApi } from "./api"

let host: Host | undefined
let server: Server | undefined
afterEach(async () => {
  host?.dispose()
  await server?.close()
  server = undefined
})

describe.skipIf(!hasDuckd)("system API against the real duckd", CONTRACT, () => {
  it("lists the compiled features with their contracts", async () => {
    host = installHost()
    const manifest = await featureManifest()
    expect(manifest.features.map((feature) => feature.id)).toEqual([
      "rkp",
      "rkp-device",
      "tee-attestation",
      "device-ids",
      "tricky-store",
      "system",
    ])
  })

  it("summarizes the module and the device", async () => {
    host = installHost({ props: DEVICE_PROPS })
    const info = await systemApi.info()
    expect(info.module.id).toBe("duck-toolbox")
    expect(info.device.model).toBe("Pixel 9 Pro")
    expect(info.device.security_patch).toBe("2026-09-05")
  })

  it("checks the stable channel against updateJson", async () => {
    const routes: Record<string, string> = {}
    server = await serve(routes)
    routes["/CHANGELOG.md"] =
      "# Changelog\n\n## v9.0.0\n\n- New things\n\n## v0.0.1\n\n- Old things\n"
    routes["/update.json"] = JSON.stringify({
      versionCode: 900,
      version: "v9.0.0",
      zipUrl: server.url("/duck-toolbox-v9.0.0.zip"),
      changelog: server.url("/CHANGELOG.md"),
    })
    const updateJson = server.url("/update.json")
    host = installHost({
      moduleProp: `id=duck-toolbox\nname=Duck ToolBox\nversion=v0.0.1\nversionCode=1\nauthor=test\ndescription=test\nupdateJson=${updateJson}\n`,
    })
    const update = await systemApi.checkUpdate("stable")
    expect(update.available).toBe(true)
    expect(update.version).toBe("v9.0.0")
    expect(update.changelog).toContain("New things")
    expect(update.changelog).not.toContain("Old things")
  })

  it("lists outputs, files and the command log", async () => {
    host = installHost({ files: { "/sdcard/Download/keybox.xml": "<AndroidAttestation/>" } })
    await systemApi.info()
    expect((await systemApi.artifacts()).outputs).toEqual([])
    const files = await listFiles("/sdcard/Download", "xml")
    expect(files.entries.map((entry) => entry.name)).toEqual(["keybox.xml"])
    const log = await systemApi.log(5)
    expect(log.entries.map((entry) => entry.command)).toContain("system.info")
  })
})
