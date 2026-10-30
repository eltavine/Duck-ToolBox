import { z } from "zod"
import { duckd } from "@/core/duckd"
import { previewSchema } from "./api"

export const archiveChannelSchema = z.enum(["release", "beta", "canary"])
export type ArchiveChannel = z.infer<typeof archiveChannelSchema>
const buildSchema = z.object({
  latest: z.string(),
  build: z.string(),
  incremental: z.string(),
  security_patch: z.string(),
  android: z.string(),
  vbmeta_digest: z.string(),
})
export const archiveCatalogSchema = z.object({
  source: z.literal("https://github.com/VictorGabriel7Dev/build.prop"),
  source_revision: z.string(),
  fetched: z.number(),
  generated: z.string(),
  devices: z.record(
    z.string(),
    z.object({
      model: z.string(),
      first_api_level: z.number(),
      channels: z.partialRecord(archiveChannelSchema, buildSchema),
    }),
  ),
})
export type ArchiveCatalog = z.infer<typeof archiveCatalogSchema>
export const pixelArchiveApi = {
  catalog: () =>
    duckd(["play-integrity", "pixel-archive-catalog"], { schema: archiveCatalogSchema }),
  preview: (catalog: ArchiveCatalog, device: string, channel: ArchiveChannel) => {
    const build = catalog.devices[device]?.channels[channel]
    if (!build) throw new Error("Select an available Pixel archive build")
    return duckd(["play-integrity", "pixel-archive-preview"], {
      input: {
        source_revision: catalog.source_revision,
        device,
        channel,
        build: build.build,
        incremental: build.incremental,
      },
      schema: previewSchema,
    })
  },
}
