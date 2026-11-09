import { fileURLToPath } from "node:url"

import { configDefaults, defineConfig, mergeConfig } from "vitest/config"

import viteConfig from "./vite.config.ts"

export default defineConfig((env) =>
  mergeConfig(viteConfig(env), {
    test: {
      environment: "happy-dom",
      include: ["src/**/*.test.ts", "scripts/**/*.test.ts"],
      exclude: [...configDefaults.exclude, "e2e/**"],
      root: fileURLToPath(new URL("./", import.meta.url)),
    },
  }),
)
