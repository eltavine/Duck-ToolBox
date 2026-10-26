import { MockFailure } from "@/core/mock"
import type { MockModule } from "@/core/mock"

import type { KernelModule } from "./api"

const scenario = new URLSearchParams(location.search).get("kernelModules")
let modules: KernelModule[] = [
  {
    name: "kernelsu",
    source: "loaded",
    size: 196608,
    references: 0,
    used_by: [],
    state: "Live",
    unload_protected: true,
  },
  {
    name: "usb_f_mass_storage",
    source: "loaded",
    size: 65536,
    references: 0,
    used_by: [],
    state: "Live",
    unload_protected: false,
  },
  {
    name: "cfg80211",
    source: "loaded",
    size: 1310720,
    references: 3,
    used_by: ["wlan"],
    state: "Live",
    unload_protected: false,
  },
  {
    name: "binder",
    source: "sysfs_only",
    size: null,
    references: null,
    used_by: [],
    state: null,
    unload_protected: false,
  },
]
const samplePath = "/data/local/tmp/sample.ko"
const mock: MockModule = {
  feature: { id: "kernel-modules", contract: 3 },
  handlers: {
    "kernel-modules.status": () => ({
      kernel_release: "6.6.30-android15",
      modules_disabled: scenario === "disabled",
      operations_supported: true,
      proc_available: scenario !== "unavailable",
      sysfs_available: true,
      warnings: scenario === "unavailable" ? ["/proc/modules: Permission denied"] : [],
      modules,
    }),
    "kernel-modules.details": (args) => {
      const module = modules.find((item) => item.name === args[0])
      if (!module) throw new MockFailure("state_unavailable", "Module disappeared")
      return {
        module,
        version: "1.0",
        srcversion: "D8A9CC",
        taint: scenario === "untainted" ? "" : scenario === "taint-unavailable" ? null : "O",
        holders: module.used_by,
        holders_available: scenario !== "holders-unavailable",
        parameters: [{ name: "debug", value: "N", error: null }],
        warnings: [],
      }
    },
    "kernel-modules.files": () => ({
      roots: ["/vendor/lib/modules", "/vendor_dlkm/lib/modules"],
      files: [{ path: "/vendor/lib/modules/sample.ko", size: 65536 }],
      warnings: [],
      truncated: false,
    }),
    "kernel-modules.inspect": (args) => {
      const path = args[0] ?? samplePath
      if (path.includes("invalid")) throw new MockFailure("invalid_input", "Invalid ELF module")
      return {
        path,
        name: path.includes("long-name") ? "a".repeat(56) : "sample",
        size: 65536,
        sha256: "ab".repeat(32),
        architecture: path.includes("arm32") ? "arm" : "aarch64",
        elf_class: path.includes("arm32") ? 32 : 64,
        little_endian: true,
        versioned_symbols: true,
        warnings: [],
        metadata: {
          license: ["GPL"],
          description: ["Example kernel module"],
          vermagic: [
            path.includes("mismatch") ? "6.1.0-other SMP" : "6.6.30-android15 SMP preempt",
          ],
          parm: ["debug:Enable debug logging"],
        },
        kernel_release_match: !path.includes("mismatch"),
      }
    },
    "kernel-modules.load": (_args, _flags, input) => {
      const request = input as { confirmed: boolean; sha256: string }
      if (!request.confirmed) throw new MockFailure("confirmation_required", "Confirm first")
      if (scenario === "reject")
        throw new MockFailure("kernel_rejected", "Required key not available")
      if (scenario === "changed")
        throw new MockFailure("file_changed", "File changed after inspection")
      modules = [
        ...modules.filter((module) => module.name !== "sample"),
        {
          name: "sample",
          source: "loaded",
          size: 65536,
          references: 0,
          used_by: [],
          state: "Live",
          unload_protected: false,
        },
      ]
      return { name: "sample", action: "load", observed_loaded: true }
    },
    "kernel-modules.unload": (_args, _flags, input) => {
      const request = input as { name: string; confirmed: boolean }
      if (!request.confirmed) throw new MockFailure("confirmation_required", "Confirm first")
      if (request.name === "kernelsu")
        throw new MockFailure("module_protected", "Protected root core")
      if (scenario === "holders-unavailable")
        throw new MockFailure("state_unavailable", "Cannot read module holders")
      modules = modules.filter((module) => module.name !== request.name)
      return { name: request.name, action: "unload", observed_loaded: false }
    },
  },
}
export default mock
