// @vitest-environment node
import { chmodSync, readFileSync, utimesSync, writeFileSync } from "node:fs"
import { execFileSync } from "node:child_process"
import path from "node:path"
import { afterEach, describe, expect, it } from "vitest"
import type { Host } from "@/core/testing/host"
import { CONTRACT, hasDuckd, installHost } from "@/core/testing/host"
import { fakerApi } from "./api"

let host: Host | undefined
afterEach(() => host?.dispose())
const content = '[[apps]]\npackage = "com.example.app@999"\nmodel = "Phone"\n'
describe.skipIf(!hasDuckd)("Device Faker real process contract", CONTRACT, () => {
  it("previews, applies, imports and restores independently of enabling", async () => {
    host = installHost({
      files: {
        "/data/adb/device_faker/config/config.toml": content,
      },
    })
    expect(await fakerApi.status()).toMatchObject({ enabled: false, injection_verified: false })
    const before = await fakerApi.config()
    const imported = await fakerApi.importPreview()
    expect(imported.summary.targets).toEqual(["com.example.app@999"])
    expect(await fakerApi.config()).toEqual(before)
    const saved = await fakerApi.apply(imported, false)
    expect(saved.content).toBe(content)
    expect((await fakerApi.status()).enabled).toBe(false)
    await expect(fakerApi.apply(imported, false)).rejects.toMatchObject({ code: "engine_error" })
    const restorePreview = await fakerApi.restorePreview()
    expect(restorePreview.content).toBe(before.content)
    expect((await fakerApi.config()).content).toBe(content)
    const restored = await fakerApi.restore(saved.revision)
    expect(restored.content).toBe(before.content)
  })
  it("blocks unacknowledged global changes and keeps stop available for broken config", async () => {
    host = installHost({ files: { "/data/adb/duck-device-faker/config/config.toml": "invalid[" } })
    const status = await fakerApi.status()
    expect(status.config.error).toBeTruthy()
    const preview = await fakerApi.preview(content + "dpi = 420\n")
    expect(preview.summary.global_effects).toBe(true)
    await expect(fakerApi.apply(preview, false)).rejects.toMatchObject({ code: "engine_error" })
    await fakerApi.apply(preview, true)
    await expect(
      fakerApi.enable((await fakerApi.config()).revision, status.stop_revision),
    ).rejects.toMatchObject({ code: "engine_error" })
    expect(await fakerApi.disable()).toMatchObject({ enabled: false })
  })
  it("refuses conflicts and stale enable tokens", async () => {
    host = installHost({
      files: {
        "/proc/stat": "btime 10\n",
        "/proc/sys/kernel/random/boot_id": "contract-boot",
        "/data/adb/modules/duck-toolbox/module.prop": "id=duck-toolbox",
        "/data/adb/modules/duck-toolbox/zygisk/arm64-v8a.so": "fixture",
        "/data/adb/modules/duck-toolbox/prop.sh":
          "#!/bin/sh\nprintf 'legacy\\n' >> \"$DF_BOOT_TEST_TRACE\"\n",
        "/data/adb/modules/duck-toolbox/play-integrity/pif-engine":
          "#!/bin/sh\nprintf 'integrity\\n' >> \"$DF_BOOT_TEST_TRACE\"\n",
        "/data/adb/modules/device_faker/module.prop": "id=device_faker",
      },
    })
    chmodSync(host.path("/data/adb/modules/duck-toolbox/play-integrity/pif-engine"), 0o755)
    const trace = host.path("/boot-stages")
    execFileSync(
      process.env.DEVICE_FAKER_ENGINE_BIN ??
        path.resolve("../target/device-faker-manager/debug/device-faker-engine"),
      ["stage", "ready"],
      { env: { ...process.env, DEVICE_FAKER_SYSROOT: host.path("/"), DF_BOOT_TEST_TRACE: trace } },
    )
    expect(readFileSync(trace, "utf8")).toBe("legacy\nintegrity\n")
    const candidate = await fakerApi.preview(content)
    const saved = await fakerApi.apply(candidate, false)
    const status = await fakerApi.status()
    await expect(fakerApi.enable(saved.revision, status.stop_revision)).rejects.toMatchObject({
      code: "engine_error",
    })
    writeFileSync(host.path("/data/adb/modules/device_faker/disable"), "")
    await expect(fakerApi.enable(saved.revision, status.stop_revision)).rejects.toMatchObject({
      code: "engine_error",
    })
    utimesSync(host.path("/data/adb/modules/device_faker/disable"), new Date(1000), new Date(1000))
    await fakerApi.enable(saved.revision, status.stop_revision)
    expect((await fakerApi.status()).enabled).toBe(true)
    const pending = host.path("/data/adb/modules_update/duck-toolbox/module.prop")
    const { mkdirSync, rmSync } = await import("node:fs")
    mkdirSync(path.dirname(pending), { recursive: true })
    writeFileSync(pending, "id=duck-toolbox")
    await expect(fakerApi.enable(saved.revision, status.stop_revision)).rejects.toMatchObject({
      details: { code: "startup_not_ready" },
    })
    rmSync(pending)
    await fakerApi.disable()
    await expect(fakerApi.enable(saved.revision, status.stop_revision)).rejects.toMatchObject({
      code: "engine_error",
    })
    expect((await fakerApi.status()).enabled).toBe(false)
  })
  it("rejects an empty power acknowledgement and allows stop to repair it", async () => {
    host = installHost({
      files: {
        "/data/adb/duck-device-faker/enabled": "",
        "/data/adb/duck-device-faker/stop": "",
        "/data/adb/duck-device-faker/config/config.toml": content,
      },
    })
    const status = await fakerApi.status()
    expect(status.enabled).toBe(false)
    await expect(fakerApi.enable(status.config.revision, "")).rejects.toMatchObject({
      details: { code: "power_changed" },
    })
    expect((await fakerApi.disable()).stop_revision).not.toBe("")
  })
  it("blocks a lost global owner and keeps its recovery marker after stop", async () => {
    host = installHost({
      files: {
        "/proc/sys/kernel/random/boot_id": "contract-boot",
        "/data/adb/duck-device-faker/enabled": "initial",
        "/data/adb/duck-device-faker/global-active": "contract-boot\n2000000000\n123\ninitial\n",
        "/data/adb/duck-device-faker/config/config.toml": content,
      },
    })
    const status = await fakerApi.status()
    expect(status).toMatchObject({
      enabled: true,
      cleanup_pending: true,
      global_recovery_requires_reboot: true,
    })
    await expect(
      fakerApi.enable(status.config.revision, status.stop_revision),
    ).rejects.toMatchObject({ code: "engine_error" })
    expect(await fakerApi.disable()).toMatchObject({ enabled: false })
    expect((await fakerApi.status()).global_recovery_requires_reboot).toBe(true)
  })
})
