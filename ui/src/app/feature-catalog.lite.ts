import type { FeatureDefinition } from "@/core/features"

const modules = import.meta.glob<{ default: FeatureDefinition }>(
  "../features/{rkp,rkp-device,tricky-store}/index.ts",
  { eager: true },
)

export default Object.values(modules).map((module) => module.default)
