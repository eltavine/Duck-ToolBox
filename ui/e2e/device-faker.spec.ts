import { expect, test } from "./fixtures"

test.use({ deviceScaleFactor: 1 })

test("imports and previews without enabling, then applies and independently controls power", async ({
  page,
  open,
}) => {
  await open("/device-faker", { locale: "en" })
  await expect(page.getByText("Disabled for new app processes", { exact: true })).toBeVisible()
  await page
    .getByRole("button", { name: "Preview existing Device Faker configuration", exact: true })
    .click()
  await expect(page.getByLabel("Configuration (TOML)", { exact: true })).not.toHaveValue(
    /com.example.legacy@999/,
  )
  await expect(page.getByText("Disabled for new app processes", { exact: true })).toBeVisible()
  await page.getByRole("button", { name: "Apply configuration", exact: true }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Apply configuration", exact: true })
    .click()
  await expect(
    page.getByRole("listitem").filter({ hasText: "com.example.legacy@999" }),
  ).toBeVisible()
  await page.getByRole("button", { name: "Enable", exact: true }).click()
  await expect(page.getByText("Enabled for new app processes", { exact: true })).toBeVisible()
  await page.getByRole("button", { name: "Stop spoofing", exact: true }).click()
  await expect(page.getByText("Disabled for new app processes", { exact: true })).toBeVisible()
  await expect(page.getByLabel("Configuration (TOML)", { exact: true })).toHaveValue(
    /com.example.legacy@999/,
  )
  await page.getByRole("button", { name: "Preview previous configuration", exact: true }).click()
  await expect(page.getByLabel("Configuration (TOML)", { exact: true })).toHaveValue(
    /com.example.legacy/,
  )
  await expect(
    page.getByRole("listitem").filter({ hasText: "com.example.legacy@999" }),
  ).toBeVisible()
  await page.getByRole("button", { name: "Apply configuration", exact: true }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Apply configuration", exact: true })
    .click()
  await expect(
    page.getByRole("listitem").filter({ hasText: "com.example.legacy@999" }),
  ).toHaveCount(0)
})

for (const locale of [
  "en",
  "zh-CN",
  "zh-TW",
  "ar",
  "de",
  "es-ES",
  "fr",
  "id",
  "it",
  "ja",
  "ko",
  "pl",
  "pt-BR",
  "ru",
  "uk",
  "vi",
] as const) {
  test(`configuration fits a narrow screen in ${locale}`, async ({ page, open }) => {
    await page.setViewportSize({ width: 320, height: 850 })
    await open("/device-faker", { locale })
    await expect(page.locator("#faker-config")).toBeVisible()
    await page.locator("summary").first().click()
    await expect(page.locator("#faker-config")).toBeVisible()
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth > window.innerWidth,
    )
    expect(overflow).toBe(false)
  })
}

for (const theme of ["light", "dark"] as const) {
  test(`enlarged text remains aligned in ${theme}`, async ({ page, open }) => {
    await page.setViewportSize({ width: 320, height: 900 })
    await open("/device-faker", { locale: "en", theme })
    await page.evaluate(() => {
      document.documentElement.style.fontSize = "200%"
    })
    await expect(page.locator("#faker-config")).toBeVisible()
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true)
    const controls = await page
      .getByRole("button", { name: "Stop spoofing", exact: true })
      .boundingBox()
    expect(controls?.height).toBeGreaterThanOrEqual(48)
    await page.screenshot({ path: `/tmp/duck-device-faker-${theme}.png`, fullPage: true })
  })
}

test("preserves the draft across navigation and keeps stop available while startup is blocked", async ({
  page,
  open,
}) => {
  await open("/device-faker", { locale: "en", query: { "faker-startup-failed": "1" } })
  const draft = '[[apps]]\npackage = "com.example.draft"\nmodel = "Draft"\n'
  await page.locator("#faker-config").fill(draft)
  await page.evaluate(() => {
    location.hash = "/system"
  })
  await expect(page.locator("#faker-config")).toHaveCount(0)
  await page.evaluate(() => {
    location.hash = "/device-faker"
  })
  await expect(page.locator("#faker-config")).toHaveValue(draft)
  await expect(
    page.getByText("Startup compatibility checks failed.", { exact: false }),
  ).toBeVisible()
  await expect(page.getByRole("button", { name: "Enable", exact: true })).toBeDisabled()
  await expect(page.getByRole("button", { name: "Stop spoofing", exact: true })).toBeEnabled()
  await page.getByRole("button", { name: "Stop spoofing", exact: true }).click()
  await expect(page.locator("#faker-config")).toHaveValue(draft)
})
