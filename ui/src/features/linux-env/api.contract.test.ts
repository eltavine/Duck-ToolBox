// @vitest-environment node
import { createHash } from "node:crypto"
import { mkdtempSync, rmSync, writeFileSync, symlinkSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"
import { afterEach, describe, expect, it } from "vitest"
import { duckd, featureManifest } from "@/core/duckd"
import { CONTRACT, hasDuckd, installHost } from "@/core/testing/host"
import type { Host } from "@/core/testing/host"
import { linuxApi, configSchema } from "./api"
let host: Host | undefined
let temporary: string | undefined
afterEach(() => {
  host?.dispose()
  host = undefined
  if (temporary) rmSync(temporary, { recursive: true, force: true })
  temporary = undefined
})
describe.skipIf(!hasDuckd)("Linux environments API against real duckd", CONTRACT, () => {
  it("exposes the feature contract and keeps a host read only", async () => {
    host = installHost()
    expect((await featureManifest()).features.find((f) => f.id === "linux-env")?.contract).toBe(1)
    const status = await linuxApi.status()
    expect(status).toMatchObject({
      supported: false,
      runtime_ready: false,
      busy: false,
      environments: [],
      job: null,
    })
    for (const action of ["start", "stop", "remove"])
      await expect(
        duckd(["linux-env", action], {
          input: { id: "debian", revision: "a".repeat(64), confirmed: true },
        }),
      ).rejects.toMatchObject({ code: "unsupported_environment" })
  })
  it("hashes local regular files and refuses symlinks and relative paths", async () => {
    host = installHost()
    temporary = mkdtempSync(path.join(tmpdir(), "linux-inspect-"))
    const file = path.join(temporary, "rootfs.tar")
    writeFileSync(file, "archive snapshot")
    expect(await linuxApi.inspect(file)).toEqual({
      path: file,
      bytes: 16,
      sha256: createHash("sha256").update("archive snapshot").digest("hex"),
    })
    symlinkSync(file, path.join(temporary, "link"))
    await expect(linuxApi.inspect(path.join(temporary, "link"))).rejects.toBeDefined()
    await expect(linuxApi.inspect("relative.tar")).rejects.toMatchObject({ code: "invalid_input" })
  })
})
describe("Linux environment configuration", () => {
  const config = {
    name: "Debian",
    auto_start: false,
    dns: [],
    services: [{ id: "web", command: "echo ready", enabled: true }],
  }
  it("enforces UTF-8 byte limits, unique IDs, NUL exclusion and DNS addresses", () => {
    expect(configSchema.safeParse(config).success).toBe(true)
    expect(configSchema.safeParse({ ...config, name: "中".repeat(41) }).success).toBe(false)
    expect(
      configSchema.safeParse({ ...config, services: [...config.services, ...config.services] })
        .success,
    ).toBe(false)
    expect(
      configSchema.safeParse({
        ...config,
        services: [{ ...config.services[0], command: "echo\0oops" }],
      }).success,
    ).toBe(false)
    expect(configSchema.safeParse({ ...config, dns: ["https://dns.example"] }).success).toBe(false)
    expect(
      configSchema.safeParse({ ...config, dns: ["1.1.1.1", "2606:4700:4700::1111"] }).success,
    ).toBe(true)
  })
})
