import { expect, test } from "./fixtures"

test("community archive previews and saves a Canary candidate without applying or enabling", async ({
  page,
  open,
}) => {
  await open("/play-integrity")
  await page.getByRole("switch", { name: "Enable the engine", exact: true }).click()
  await expect(
    page.getByRole("switch", { name: "Enable the engine", exact: true }),
  ).not.toBeChecked()
  await page.getByRole("link", { name: "Pixel property library", exact: true }).click()
  await page.getByRole("button", { name: "Fetch property library", exact: true }).click()
  await expect(page.getByRole("combobox", { name: "Pixel device", exact: true })).toHaveText(
    "Pixel 6a",
  )
  await page.getByRole("button", { name: "Source details", exact: true }).click()
  await expect(page.getByText("Boot hash (reference)", { exact: true })).toBeVisible()
  await page.getByRole("combobox", { name: "Release channel", exact: true }).click()
  await page.getByRole("option", { name: "Canary", exact: true }).click()
  await page.getByRole("button", { name: "Preview archive configuration", exact: true }).click()
  await expect(
    page.getByRole("cell", {
      name: "google/bluejay_beta/bluejay:CANARY/CANARY.1/5678:user/release-keys",
      exact: true,
    }),
  ).toBeVisible()
  await expect(page.getByRole("cell", { name: "ro.boot.vbmeta.digest", exact: true })).toHaveCount(
    0,
  )
  await page.getByLabel("Configuration name", { exact: true }).fill("Archived Canary")
  await page.getByRole("button", { name: "Save draft as snapshot", exact: true }).click()
  await page.getByRole("button", { name: "Dismiss preview", exact: true }).click()
  await page.getByRole("link", { name: "Configuration", exact: true }).click()
  await expect(page.getByLabel("Device model · MODEL", { exact: true })).toHaveValue("Pixel 9 Pro")
  await page.getByRole("button", { name: "Saved configurations", exact: true }).click()
  await page.getByRole("button", { name: "Preview changes: Archived Canary", exact: true }).click()
  await expect(
    page.getByText(/https:\/\/github.com\/VictorGabriel7Dev\/build.prop · CANARY.1/),
  ).toBeVisible()
  await page.getByRole("link", { name: "Status", exact: true }).click()
  await expect(
    page.getByRole("switch", { name: "Enable the engine", exact: true }),
  ).not.toBeChecked()
})

test("changing archive device clears the preview and selects an available channel", async ({
  page,
  open,
}) => {
  await open("/play-integrity/archive")
  await page.getByRole("button", { name: "Fetch property library", exact: true }).click()
  await page.getByRole("combobox", { name: "Release channel", exact: true }).click()
  await page.getByRole("option", { name: "Beta", exact: true }).click()
  await page.getByRole("button", { name: "Preview archive configuration", exact: true }).click()
  await expect(page.getByRole("button", { name: "Apply configuration", exact: true })).toBeVisible()
  await page.getByRole("combobox", { name: "Pixel device", exact: true }).click()
  await page.getByRole("option", { name: "Pixel 7a", exact: true }).click()
  await expect(page.getByRole("button", { name: "Apply configuration", exact: true })).toHaveCount(
    0,
  )
  await expect(page.getByRole("combobox", { name: "Release channel", exact: true })).toHaveText(
    "Stable",
  )
  await page.getByRole("combobox", { name: "Release channel", exact: true }).click()
  await expect(page.getByRole("option", { name: "Beta", exact: true })).toBeDisabled()
})

test("archive outage is recoverable and preserves the saved configuration", async ({
  page,
  open,
}) => {
  await open("/play-integrity/archive", { query: { "archive-offline": "1" } })
  await page.getByRole("button", { name: "Fetch property library", exact: true }).click()
  await expect(page.getByRole("alert").filter({ hasText: "Engine operation failed" })).toBeVisible()
  await page.getByRole("button", { name: "Details", exact: true }).click()
  await expect(page.getByRole("dialog")).toContainText("Archive unavailable")
  await page.keyboard.press("Escape")
  await expect(
    page.getByRole("button", { name: "Fetch property library", exact: true }),
  ).toBeEnabled()
  await expect(
    page.getByRole("button", { name: "Preview archive configuration", exact: true }),
  ).toHaveCount(0)
  await page.getByRole("link", { name: "Configuration", exact: true }).click()
  await expect(page.getByLabel("Device model · MODEL", { exact: true })).toHaveValue("Pixel 9 Pro")
})

for (const [locale, fetch, preview] of [
  ["en", "Fetch property library", "Preview archive configuration"],
  ["zh-CN", "获取属性库", "预览属性库配置"],
  ["ar", "جلب مكتبة الخصائص", "معاينة إعدادات الأرشيف"],
] as const) {
  test(`archive configuration fits a narrow viewport in ${locale}`, async ({
    page,
    open,
  }, testInfo) => {
    await page.setViewportSize({ width: 320, height: 800 })
    await open("/play-integrity/archive", { locale })
    await page.getByRole("button", { name: fetch, exact: true }).click()
    await page.getByRole("button", { name: preview, exact: true }).click()
    await expect(
      page.getByRole("cell", { name: "DEVICE_INITIAL_SDK_INT", exact: true }),
    ).toBeVisible()
    const size = await page.evaluate(() => ({
      width: innerWidth,
      scroll: document.documentElement.scrollWidth,
    }))
    expect(size.scroll).toBeLessThanOrEqual(size.width)
    await page.screenshot({
      path: testInfo.outputPath("pixel-property-library.png"),
      fullPage: true,
    })
  })
}

test("archive controls have 48px touch targets and align in a dark desktop layout", async ({
  page,
  open,
}, testInfo) => {
  await page.setViewportSize({ width: 1024, height: 900 })
  await open("/play-integrity/archive", { theme: "dark" })
  await page.getByRole("button", { name: "Fetch property library", exact: true }).click()
  const names = ["Fetch property library", "Preview archive configuration", "Source details"]
  for (const name of names) {
    const size = await page.getByRole("button", { name, exact: true }).boundingBox()
    expect(size!.height).toBeGreaterThanOrEqual(48)
  }
  const device = await page
    .getByRole("combobox", { name: "Pixel device", exact: true })
    .boundingBox()
  const channel = await page
    .getByRole("combobox", { name: "Release channel", exact: true })
    .boundingBox()
  expect(device!.x).toBe(channel!.x)
  expect(device!.width).toBe(channel!.width)
  await page.getByRole("button", { name: "Source details", exact: true }).click()
  await expect(page.getByText("Boot hash (reference)", { exact: true })).toBeVisible()
  await page.screenshot({ path: testInfo.outputPath("pixel-library-dark.png"), fullPage: true })
})

for (const [locale, fetch, preview, preparing] of [
  ["en", "Fetch property library", "Preview archive configuration", "Preparing configuration…"],
  ["zh-CN", "获取属性库", "预览属性库配置", "正在准备配置…"],
] as const) {
  test(`archive preview announces its own progress in ${locale}`, async ({ page, open }) => {
    await open("/play-integrity/archive", { locale, query: { latency: "400" } })
    await page.getByRole("button", { name: fetch, exact: true }).click()
    await page.getByRole("button", { name: preview, exact: true }).click()
    await expect(page.getByRole("status").filter({ hasText: preparing })).toBeVisible()
    await expect(page.getByRole("button", { name: fetch, exact: true })).toBeDisabled()
    await expect(page.getByRole("button", { name: preparing, exact: true })).toBeDisabled()
    await expect(page.getByRole("button", { name: preview, exact: true })).toBeEnabled()
  })
}
