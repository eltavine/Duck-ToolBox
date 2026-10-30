import { expect, test } from "./fixtures"

test.describe("Canary date-only metadata", () => {
  test.use({ timezoneId: "America/Los_Angeles" })
  for (const [locale, generate, released, expiry] of [
    ["en", "Generate fingerprint", "Oct 1, 2026", "Nov 12, 2026"],
    ["zh-CN", "生成指纹", "2026年10月1日", "2026年11月12日"],
    ["ar", "إنشاء بصمة", "1 أكتوبر 2026", "12 نوفمبر 2026"],
  ] as const) {
    test(`localizes dates without shifting the release day in ${locale}`, async ({
      page,
      open,
    }, testInfo) => {
      await page.setViewportSize({ width: 320, height: 800 })
      await open("/play-integrity/generate", { locale })
      await page.getByRole("button", { name: generate, exact: true }).click()
      await expect(page.getByText(released, { exact: true })).toBeVisible()
      await expect(page.getByText(expiry, { exact: true })).toBeVisible()
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
        320,
      )
      await page.screenshot({ path: testInfo.outputPath("canary-dates.png"), fullPage: true })
    })
  }
})

test("searches new, fold, tablet and legacy Pixel models by name or codename", async ({
  page,
  open,
}) => {
  await open("/play-integrity/generate")
  const search = page.getByRole("searchbox", { name: "Search model or codename", exact: true })
  const device = page.getByRole("combobox", { name: "Pixel device", exact: true })
  for (const [query, model] of [
    [" YOGI ", "Pixel 11 Pro Fold"],
    ["10a", "Pixel 10a"],
    ["felix", "Pixel Fold"],
    ["Tablet", "Pixel Tablet"],
    ["sailfish", "Pixel"],
  ]) {
    await search.fill(query!)
    await device.click()
    await expect(page.getByRole("option")).toHaveCount(1)
    await page.keyboard.press("ArrowDown")
    await page.keyboard.press("Enter")
    await expect(device).toHaveText(model!)
  }
  await search.fill("unknown-device")
  await expect(page.getByRole("status").filter({ hasText: "No matching devices" })).toBeVisible()
  await expect(device).toBeDisabled()
  await expect(device).toHaveText("Pixel")
  await search.fill("")
  await expect(device).toBeEnabled()
})

test("changing device, incremental or refetching invalidates the previous Pixel candidate", async ({
  page,
  open,
}) => {
  await open("/play-integrity/generate")
  const fetch = page.getByRole("button", { name: "Fetch Pixel builds", exact: true })
  const preview = page.getByRole("button", { name: "Preview selected fingerprint", exact: true })
  const apply = page.getByRole("button", { name: "Apply configuration", exact: true })
  await fetch.click()
  await preview.click()
  await expect(apply).toBeVisible()
  await page
    .getByRole("searchbox", { name: "Search model or codename", exact: true })
    .fill("stallion")
  await page.getByRole("combobox", { name: "Pixel device", exact: true }).click()
  await page.getByRole("option", { name: "Pixel 10a · stallion", exact: true }).click()
  await expect(apply).toHaveCount(0)
  await expect(preview).toHaveCount(0)
  await fetch.click()
  await preview.click()
  await expect(page.getByRole("cell", { name: "Pixel 10a", exact: true }).first()).toBeVisible()
  await expect(apply).toBeVisible()
  await page.getByRole("combobox", { name: "Release build", exact: true }).click()
  await page.getByRole("option", { name: "RELEASE.1 / 1233 · Android 16", exact: true }).click()
  await expect(apply).toHaveCount(0)
  await preview.click()
  await expect(
    page.getByRole("cell", {
      name: "google/stallion/stallion:16/RELEASE.1/1233:user/release-keys",
      exact: true,
    }),
  ).toBeVisible()
  await fetch.click()
  await expect(apply).toHaveCount(0)
})

test("fetches Pixel 6a by default and saves a candidate without applying", async ({
  page,
  open,
}) => {
  await open("/play-integrity/generate")
  await expect(page.getByRole("combobox", { name: "Pixel device", exact: true })).toHaveText(
    "Pixel 6a",
  )
  await page.getByRole("button", { name: "Fetch Pixel builds", exact: true }).click()
  await expect(
    page.getByText("google/bluejay/bluejay:16/RELEASE.1/1234:user/release-keys", { exact: true }),
  ).toBeVisible()
  await page.getByRole("button", { name: "Preview selected fingerprint", exact: true }).click()
  await expect(page.getByText(/candidate leaves it unset/)).toBeVisible()
  await page.getByLabel("Configuration name", { exact: true }).fill("Pixel 6a release")
  await page.getByRole("button", { name: "Save draft as snapshot", exact: true }).click()
  await page.getByRole("button", { name: "Dismiss preview", exact: true }).click()
  await page.getByRole("link", { name: "Configuration", exact: true }).click()
  await expect(page.getByLabel("Device model · MODEL", { exact: true })).toHaveValue("Pixel 9 Pro")
  await page.getByRole("button", { name: "Saved configurations", exact: true }).click()
  await expect(page.getByText("Pixel 6a release", { exact: true })).toBeVisible()
})

for (const [locale, fetch, preview, warning] of [
  [
    "en",
    "Fetch Pixel builds",
    "Preview selected fingerprint",
    "This older source record identifies only the device and build.",
  ],
  ["zh-CN", "获取 Pixel 构建", "预览所选指纹", "此旧版来源记录仅包含设备代号和构建号"],
  [
    "ar",
    "جلب إصدارات Pixel",
    "معاينة البصمة المحددة",
    "يحدد سجل المصدر القديم هذا الجهاز والإصدار فقط.",
  ],
] as const) {
  test(`keeps legacy Pixel provenance readable and identified in ${locale}`, async ({
    page,
    open,
  }, testInfo) => {
    await page.setViewportSize({ width: 320, height: 800 })
    await open("/play-integrity/generate", { locale, query: { "legacy-pixel-source": "1" } })
    await page.getByRole("button", { name: fetch, exact: true }).click()
    await page.getByRole("button", { name: preview, exact: true }).click()
    await expect(page.getByRole("status").filter({ hasText: warning })).toBeVisible()
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(320)
    await page.screenshot({ path: testInfo.outputPath("legacy-pixel-source.png"), fullPage: true })
  })
}

test("prepares three overrides in a draft and shows pending process activation after apply", async ({
  page,
  open,
}) => {
  await open("/play-integrity/config")
  await page.getByRole("button", { name: "Prepare all three overrides", exact: true }).click()
  await page.getByRole("button", { name: "Preview changes", exact: true }).click()
  await expect(page.getByRole("cell", { name: "spoofVendingFinger", exact: true })).toBeVisible()
  await page.getByRole("button", { name: "Apply configuration", exact: true }).click()
  await expect(
    page
      .getByText("Configuration saved. Restart PI processes to load the new values.", {
        exact: true,
      })
      .first(),
  ).toBeVisible()
  await page.getByRole("link", { name: "Status", exact: true }).click()
  await expect(
    page
      .getByText("Configuration saved. Restart PI processes to load the new values.", {
        exact: true,
      })
      .first(),
  ).toBeVisible()
})

test("partition changes require another preview before synchronization", async ({ page, open }) => {
  await open("/play-integrity/config")
  await page.getByRole("button", { name: "Preview changes", exact: true }).click()
  const sync = page.getByRole("checkbox", { name: /Also apply this keystore patch policy/ })
  await page.getByRole("textbox", { name: "Vendor patch", exact: true }).fill("2026-09-05")
  await expect(sync).toBeDisabled()
  await page.getByRole("button", { name: "Update patch preview", exact: true }).click()
  await expect(sync).toBeEnabled()
  await sync.check()
  await page.getByRole("button", { name: "Apply configuration", exact: true }).click()
  await expect(page.getByText(/Reboot the device to finish activation/).first()).toBeVisible()
  await page.getByRole("link", { name: "Diagnostics", exact: true }).click()
  await page.getByRole("button", { name: "Engine logs and process restart", exact: true }).click()
  await page.getByRole("button", { name: "Restart PI processes", exact: true }).first().click()
  await expect(page.getByText(/Reboot the device to finish activation/).first()).toBeVisible()
})

test("imports decoded report without treating omitted labels as failed", async ({ page, open }) => {
  await open("/play-integrity/diagnostics")
  await page.getByRole("button", { name: "Test history", exact: true }).click()
  await page.getByRole("button", { name: "Import decoded report", exact: true }).click()
  await page.getByLabel("Decoded integrity report JSON", { exact: true }).fill(
    JSON.stringify({
      requestDetails: { requestPackageName: "example.checker" },
      deviceIntegrity: { deviceRecognitionVerdict: ["MEETS_DEVICE_INTEGRITY"] },
    }),
  )
  await page.getByRole("button", { name: "Import decoded report", exact: true }).last().click()
  await expect(page.getByText("example.checker", { exact: true })).toBeVisible()
  await expect(page.getByText(/Strong: Not returned · reason unknown/)).toBeVisible()
})

for (const [locale, fetch, preview] of [
  ["en", "Fetch Pixel builds", "Preview selected fingerprint"],
  ["zh-CN", "获取 Pixel 构建", "预览所选指纹"],
  ["ar", "جلب إصدارات Pixel", "معاينة البصمة المحددة"],
] as const) {
  test(`Pixel candidate fits a narrow viewport in ${locale}`, async ({ page, open }, testInfo) => {
    await page.setViewportSize({ width: 320, height: 800 })
    await open("/play-integrity/generate", { locale })
    await page.getByRole("button", { name: fetch, exact: true }).click()
    await page.getByRole("button", { name: preview, exact: true }).click()
    await expect(page.getByRole("cell", { name: "FINGERPRINT", exact: true })).toBeVisible()
    const size = await page.evaluate(() => ({
      width: innerWidth,
      scroll: document.documentElement.scrollWidth,
    }))
    expect(size.scroll).toBeLessThanOrEqual(size.width)
    await page.screenshot({ path: testInfo.outputPath("pixel-fingerprints.png"), fullPage: true })
  })
}

test("keeps editable SDK and custom fingerprint controls visible while clearing them", async ({
  page,
  open,
}) => {
  await open("/play-integrity/config")
  const mode = page.getByRole("combobox", { name: "Play Store override mode", exact: true })
  await mode.click()
  await page.getByRole("option", { name: "SDK compatibility ceiling", exact: true }).click()
  const sdk = page.getByRole("textbox", { name: "SDK compatibility ceiling", exact: true })
  await sdk.fill("")
  await expect(sdk).toBeVisible()
  await sdk.fill("35")
  await mode.click()
  await page.getByRole("option", { name: "Custom Play Store fingerprint", exact: true }).click()
  const fingerprint = page.getByRole("textbox", {
    name: "Custom Play Store fingerprint",
    exact: true,
  })
  await fingerprint.fill("")
  await expect(fingerprint).toBeVisible()
  await fingerprint.fill("google/custom/custom:16/RELEASE/123:user/release-keys")
  await page.getByRole("button", { name: "Preview changes", exact: true }).click()
  await expect(
    page.getByRole("cell", {
      name: "google/custom/custom:16/RELEASE/123:user/release-keys",
      exact: true,
    }),
  ).toBeVisible()
})
