import { z } from "zod"
import { duckd } from "@/core/duckd"
import { previewSchema } from "./api"
export const pixelCatalogSchema = z.object({
  device: z.string(),
  model: z.string(),
  source: z.literal("https://flash.android.com/"),
  fetched: z.number(),
  builds: z.array(
    z.object({
      id: z.string(),
      incremental: z.string(),
      release: z.string(),
      notes: z.string(),
      fingerprint: z.string(),
    }),
  ),
})
export const pixelApi = {
  devices: () =>
    duckd(["play-integrity", "pixel-devices"], {
      schema: z.object({
        default: z.string(),
        devices: z.array(z.object({ id: z.string(), name: z.string() })),
      }),
    }),
  builds: (device: string) =>
    duckd(["play-integrity", "pixel-builds"], { input: { device }, schema: pixelCatalogSchema }),
  preview: (device: string, build: string, incremental?: string) =>
    duckd(["play-integrity", "pixel-preview"], {
      input: { device, build, incremental },
      schema: previewSchema,
    }),
}
