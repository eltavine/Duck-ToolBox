import { defineConfig, devices } from "@playwright/test"

const port = 4180
const litePort = 4181

export default defineConfig({
  testDir: "e2e",
  outputDir: "test-results",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: `http://127.0.0.1:${port}/`,
    trace: "retain-on-failure",
    // A phone-sized WebView, like the KernelSU manager's WebUI activity.
    ...devices["Pixel 7"],
  },
  projects: [
    { name: "webview", testIgnore: "**/lite.spec.ts", use: { browserName: "chromium" } },
    {
      name: "lite-webview",
      testMatch: "**/lite.spec.ts",
      use: { browserName: "chromium", baseURL: `http://127.0.0.1:${litePort}/` },
    },
  ],
  webServer: [
    {
      // The e2e mode build bundles the mock host that stands in for `window.ksu`.
      command: `vite build --mode e2e --outDir dist-e2e && vite preview --outDir dist-e2e --host 127.0.0.1 --port ${port} --strictPort`,
      url: `http://127.0.0.1:${port}/`,
      reuseExistingServer: !process.env.CI,
      timeout: 180_000,
    },
    {
      command: `vite build --mode lite --outDir ../target/lite/e2e-webroot && vite preview --mode lite --outDir ../target/lite/e2e-webroot --host 127.0.0.1 --port ${litePort} --strictPort`,
      url: `http://127.0.0.1:${litePort}/`,
      reuseExistingServer: !process.env.CI,
      timeout: 180_000,
    },
  ],
})
