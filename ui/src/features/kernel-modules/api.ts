import { z } from "zod"

import { duckd } from "@/core/duckd"

const moduleSchema = z.object({
  name: z.string(),
  source: z.enum(["loaded", "sysfs_only"]),
  size: z.number().nullable(),
  references: z.number().nullable(),
  used_by: z.array(z.string()),
  state: z.string().nullable(),
  unload_protected: z.boolean(),
})
export type KernelModule = z.infer<typeof moduleSchema>

const statusSchema = z.object({
  kernel_release: z.string().nullable(),
  modules_disabled: z.boolean().nullable(),
  operations_supported: z.boolean(),
  proc_available: z.boolean(),
  sysfs_available: z.boolean(),
  warnings: z.array(z.string()),
  modules: z.array(moduleSchema),
})

const detailsSchema = z.object({
  module: moduleSchema,
  version: z.string().nullable(),
  srcversion: z.string().nullable(),
  taint: z.string().nullable(),
  holders: z.array(z.string()),
  holders_available: z.boolean(),
  parameters: z.array(
    z.object({ name: z.string(), value: z.string().nullable(), error: z.string().nullable() }),
  ),
  warnings: z.array(z.string()),
})

const fileSchema = z.object({
  path: z.string(),
  name: z.string(),
  size: z.number(),
  sha256: z.string(),
  architecture: z.enum(["aarch64", "arm", "x86_64", "i386", "riscv64", "riscv32", "other"]),
  elf_class: z.union([z.literal(32), z.literal(64)]),
  little_endian: z.boolean(),
  versioned_symbols: z.boolean(),
  warnings: z.array(z.enum(["metadata_text_replaced"])),
  metadata: z.record(z.string(), z.array(z.string())),
  kernel_release_match: z.boolean().nullable(),
})
export type ModuleFile = z.infer<typeof fileSchema>

const catalogSchema = z.object({
  roots: z.array(z.string()),
  files: z.array(z.object({ path: z.string(), size: z.number() })),
  warnings: z.array(z.string()),
  truncated: z.boolean(),
})

const operationSchema = z.object({
  name: z.string(),
  action: z.enum(["load", "unload"]),
  observed_loaded: z.boolean().nullable(),
})

export const kernelModulesApi = {
  status: () => duckd(["kernel-modules", "status"], { schema: statusSchema }),
  details: (name: string) => duckd(["kernel-modules", "details", name], { schema: detailsSchema }),
  files: () => duckd(["kernel-modules", "files"], { schema: catalogSchema }),
  inspect: (path: string) => duckd(["kernel-modules", "inspect", path], { schema: fileSchema }),
  load: (file: ModuleFile, parameters: string) =>
    duckd(["kernel-modules", "load"], {
      input: { path: file.path, sha256: file.sha256, parameters, confirmed: true },
      schema: operationSchema,
    }),
  unload: (name: string) =>
    duckd(["kernel-modules", "unload"], {
      input: { name, confirmed: true },
      schema: operationSchema,
    }),
}
