// @vitest-environment node
import { afterEach, describe, expect, it } from "vitest"

import { duckd, featureManifest } from "@/core/duckd"
import type { Host } from "@/core/testing/host"
import { CONTRACT, hasDuckd, installHost } from "@/core/testing/host"

import { kernelModulesApi } from "./api"

let host: Host | undefined
afterEach(() => {
  host?.dispose()
  host = undefined
})

describe.skipIf(!hasDuckd)("kernel modules API against the real duckd", CONTRACT, () => {
  it("distinguishes loaded modules from sysfs-only entries and reads parameters", async () => {
    host = installHost({
      files: {
        "/proc/modules": "sample 4096 0 - Live 0x0\n",
        "/proc/sys/kernel/osrelease": "6.6.30-android15\n",
        "/proc/sys/kernel/modules_disabled": "0\n",
        "/sys/module/sample/parameters/debug": "N\n",
        "/sys/module/builtin/parameters/enabled": "Y\n",
      },
    })
    const status = await kernelModulesApi.status()
    expect(
      (await featureManifest()).features.find((feature) => feature.id === "kernel-modules")
        ?.contract,
    ).toBe(2)
    expect(status.modules.map((item) => [item.name, item.source])).toEqual([
      ["builtin", "sysfs_only"],
      ["sample", "loaded"],
    ])
    expect(status.operations_supported).toBe(false)
    expect(status.kernel_release).toBe("6.6.30-android15")
    expect((await kernelModulesApi.details("sample")).parameters).toEqual([
      { name: "debug", value: "N", error: null },
    ])
  })

  it("lists .ko files and reports invalid ELF input", async () => {
    host = installHost({ files: { "/vendor/lib/modules/invalid.ko": "not ELF" } })
    expect((await kernelModulesApi.files()).files).toEqual([
      { path: "/vendor/lib/modules/invalid.ko", size: 7 },
    ])
    await expect(kernelModulesApi.inspect("/vendor/lib/modules/invalid.ko")).rejects.toMatchObject({
      code: "invalid_input",
    })
  })

  it("reads the no-unload proc format without discarding the inventory", async () => {
    host = installHost({ files: { "/proc/modules": "fixed 4096 - - Live 0x0\n" } })
    const status = await kernelModulesApi.status()
    expect(status.proc_available).toBe(true)
    expect(status.modules[0]).toMatchObject({ name: "fixed", source: "loaded", references: null })
  })

  it("retains explicit confirmation and prevents host mutations", async () => {
    host = installHost()
    await expect(
      duckd(["kernel-modules", "unload"], {
        input: { name: "sample", confirmed: false },
      }),
    ).rejects.toMatchObject({ code: "confirmation_required" })
    await expect(kernelModulesApi.unload("sample")).rejects.toMatchObject({
      code: "unsupported_environment",
    })
  })
})
