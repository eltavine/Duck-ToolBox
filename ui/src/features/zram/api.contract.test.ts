// @vitest-environment node
import { afterEach, describe, expect, it } from "vitest"
import { duckd, featureManifest } from "@/core/duckd"
import type { Host } from "@/core/testing/host"
import { CONTRACT, hasDuckd, installHost } from "@/core/testing/host"
import { zramApi } from "./api"
let host: Host | undefined
afterEach(() => {
  host?.dispose()
  host = undefined
})
const deviceFiles = {
  "/proc/meminfo": "MemTotal: 8388608 kB\nMemAvailable: 4194304 kB\n",
  "/proc/vmstat": "pswpin 12\npswpout 34\n",
  "/proc/swaps": "Filename Type Size Used Priority\n/dev/block/zram0 partition 1048572 2048 -2\n",
  "/proc/sys/kernel/random/boot_id": "12345678-1234-1234-1234-123456789abc\n",
  "/proc/self/mountinfo": "1 0 0:1 / / rw - rootfs rootfs rw\n",
  "/sys/block/zram0/ro": "0\n",
  "/sys/block/zram0/dev": "254:0\n",
  "/sys/block/zram0/disksize": "1073741824\n",
  "/sys/block/zram0/initstate": "1\n",
  "/sys/block/zram0/comp_algorithm": "lzo [lz4] zstd\n",
  "/sys/block/zram0/mm_stat": "2097152 524288 655360 0 1048576 0 0 0 0\n",
  "/sys/block/zram0/backing_dev": "none\n",
  "/sys/block/zram0/holders/.keep": "",
}

describe.skipIf(!hasDuckd)("ZRAM API against real duckd", CONTRACT, () => {
  it("reads native statistics and safely exposes an unidentifiable device", async () => {
    host = installHost({ files: deviceFiles })
    const status = await zramApi.status()
    expect((await featureManifest()).features.find((f) => f.id === "zram")?.contract).toBe(2)
    expect(status.memory_total).toBe(8 * 1073741824)
    expect(status.swap_in_pages).toBe(12)
    expect(status.swaps[0]?.used).toBe(2097152)
    expect(status.operations_supported).toBe(false)
    expect(status.devices[0]?.generation).toMatch(/^[a-f0-9]{64}$/)
    expect(status.devices[0]?.algorithm).toBe("lz4")
    expect(status.devices[0]?.stats.memory_used).toBe(655360)
    expect(status.devices[0]?.blocked).toEqual(
      expect.arrayContaining(["device_node", "swap_identity", "non_swap", "holders"]),
    )
    expect(status.saved.profiles).toEqual([])
  })
  it("protects read-only devices and rejects unknown read-only state", async () => {
    host = installHost({ files: { ...deviceFiles, "/sys/block/zram0/ro": "1\n" } })
    expect((await zramApi.status()).devices[0]?.blocked).toContain("read_only")
    host.dispose()
    host = installHost({ files: { ...deviceFiles, "/sys/block/zram0/ro": "2\n" } })
    const status = await zramApi.status()
    expect(status.devices).toEqual([])
    expect(status.warnings.join(" ")).toContain("read-only state")
  })
  it.each(["9007199254740992", "9007199254740993", "18446744073709551615"])(
    "rejects a memory limit that cannot be represented exactly: %s",
    async (limit) => {
      host = installHost({
        files: { ...deviceFiles, "/sys/block/zram0/mm_stat": `0 0 0 ${limit} 0 0 0 0 0\n` },
      })
      await expect(zramApi.status()).rejects.toMatchObject({ code: "contract_mismatch" })
    },
  )
  it("retains explicit confirmation and rejects malformed input before worker startup", async () => {
    host = installHost()
    await expect(
      duckd(["zram", "apply"], {
        input: {
          device: "zram0",
          revision: "a".repeat(64),
          action: "apply",
          config: { size_bytes: 536870912, algorithm: "lz4", priority: null, memory_limit: 0 },
          confirmed: false,
        },
      }),
    ).rejects.toMatchObject({ code: "confirmation_required" })
    await expect(
      duckd(["zram", "apply"], {
        input: { device: "zram0", action: "restore", confirmed: true, revision: "a".repeat(64) },
      }),
    ).rejects.toMatchObject({ code: "invalid_input" })
    await expect(
      duckd(["zram", "restore"], {
        input: {
          device: "zram0",
          action: "restore",
          confirmed: true,
          revision: "a".repeat(64),
          force: true,
        },
      }),
    ).rejects.toMatchObject({ code: "invalid_input" })
  })
})
