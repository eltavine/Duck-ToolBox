import { expect, test } from "./fixtures"
import { readFileSync } from "node:fs"

test.use({ deviceScaleFactor: 1 })

for (const locale of ["en", "zh-CN", "ar"]) {
  test(`profile fields preserve text direction and alignment in ${locale}`, async ({
    page,
    open,
  }) => {
    const messages = JSON.parse(
      readFileSync(
        new URL(`../src/features/device-faker/locales/${locale}.json`, import.meta.url),
        "utf8",
      ),
    ) as {
      addProfile: string
      fields: Record<string, string>
      packages: string
      append: string
    }
    await page.setViewportSize({ width: 320, height: 900 })
    await open("/device-faker", { locale })
    await page.evaluate(() => {
      document.documentElement.style.fontSize = "200%"
    })
    const builder = page
      .locator("details")
      .filter({ has: page.locator("summary", { hasText: messages.addProfile }) })
    await builder.locator("summary").click()
    await builder.getByLabel(messages.packages, { exact: true }).fill("com.example.phone")
    for (const field of ["manufacturer", "brand", "model"]) {
      const input = builder.getByLabel(messages.fields[field]!, { exact: true })
      await input.fill("هاتف 12")
      await expect(input).toHaveAttribute("dir", "auto")
      expect(await input.evaluate((node) => getComputedStyle(node).direction)).toBe("rtl")
      expect((await input.boundingBox())!.height).toBeGreaterThanOrEqual(48)
    }
    for (const field of ["device", "product", "fingerprint"]) {
      await expect(builder.getByLabel(messages.fields[field]!, { exact: true })).toHaveAttribute(
        "dir",
        "ltr",
      )
    }
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
    if (locale === "ar") {
      await expect(builder.locator("p").first()).toContainText("\u2066@0\u2069")
      await expect(builder.locator("p").first()).toContainText("\u2066@999\u2069")
      await builder.screenshot({ path: "/tmp/duck-faker-builder-ar.png", animations: "disabled" })
    }
    await builder.getByRole("button", { name: messages.append, exact: true }).click()
    await expect(page.locator("#faker-config")).toHaveValue(/model = "هاتف 12"/)
  })
}

for (const locale of ["en", "zh-CN", "ar"]) {
  for (const theme of ["light", "dark"] as const) {
    test(`saved configuration errors remain actionable in ${locale} / ${theme}`, async ({
      page,
      open,
    }) => {
      await page.setViewportSize({ width: 320, height: 900 })
      await open("/device-faker", { locale, theme, query: { "faker-invalid-config": "1" } })
      await page.evaluate(() => {
        document.documentElement.style.fontSize = "200%"
      })
      const alert = page.getByRole("alert").filter({ has: page.locator("details") })
      const details = alert.locator("summary")
      await details.focus()
      await page.keyboard.press("Enter")
      const diagnostic = alert.locator("pre")
      await expect(diagnostic).toBeVisible()
      await expect(diagnostic).toContainText("line 1, column 8")
      await expect(diagnostic).toHaveAttribute("dir", "ltr")
      expect((await details.boundingBox())!.height).toBeGreaterThanOrEqual(48)
      await expect(page.locator("#faker-config")).toHaveValue("invalid[")
      await expect(page.locator("#faker-config")).toHaveAttribute(
        "aria-describedby",
        "faker-property-help",
      )
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
      )
      if (locale === "ar") {
        await alert.screenshot({
          path: `/tmp/duck-faker-config-error-ar-${theme}.png`,
          animations: "disabled",
        })
      }
    })
  }
}

for (const locale of ["en", "zh-CN", "ar"]) {
  test(`mixed template names and global confirmation remain readable in ${locale}`, async ({
    page,
    open,
  }) => {
    const messages = JSON.parse(
      readFileSync(
        new URL(`../src/features/device-faker/locales/${locale}.json`, import.meta.url),
        "utf8",
      ),
    ) as Record<string, string>
    await page.setViewportSize({ width: 320, height: 900 })
    await open("/device-faker", { locale })
    await page.evaluate(() => {
      document.documentElement.style.fontSize = "200%"
    })
    await page
      .locator("#faker-config")
      .fill(
        '[templates."هاتف 12"]\npackages=["com.example.a"]\nmodel="Phone"\ndpi=420\n[templates."Pixel 9"]\npackages=["com.example.b"]\nmodel="Phone"\n',
      )
    await page.getByRole("button", { name: messages.preview!, exact: true }).click()
    const preview = page.getByRole("region", { name: messages.preview!, exact: true })
    await expect(preview.locator("bdi")).toHaveCount(2)
    await expect(preview.locator("bdi").first()).toHaveAttribute("dir", "auto")
    await preview.getByRole("button", { name: messages.apply!, exact: true }).click()
    const dialog = page.getByRole("alertdialog")
    await expect(dialog).toBeVisible()
    const confirm = dialog.getByRole("button", { name: messages.apply!, exact: true })
    await confirm.scrollIntoViewIfNeeded()
    const box = await confirm.boundingBox()
    expect(box!.height).toBeGreaterThanOrEqual(48)
    expect(box!.x).toBeGreaterThanOrEqual(0)
    expect(box!.x + box!.width).toBeLessThanOrEqual(320)
    expect(await dialog.evaluate((node) => node.scrollWidth <= node.clientWidth)).toBe(true)
    const header = await dialog.locator('[data-slot="alert-dialog-header"]').boundingBox()
    const footer = await dialog.locator('[data-slot="alert-dialog-footer"]').boundingBox()
    expect(header!.y + header!.height).toBeLessThanOrEqual(footer!.y)
    if (locale === "ar") {
      await page.screenshot({ path: "/tmp/duck-faker-confirm-ar.png", animations: "disabled" })
    }
    await confirm.click()
    await expect(dialog).toHaveCount(0)
  })
}

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
  await expect(page.getByRole("button", { name: "Enable", exact: true })).toBeDisabled()
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
  await expect(page.getByText("Wait for toolbox startup to finish.", { exact: false })).toHaveCount(
    0,
  )
  await expect(page.getByRole("button", { name: "Enable", exact: true })).toBeDisabled()
  await expect(page.getByRole("button", { name: "Stop spoofing", exact: true })).toBeEnabled()
  await page.getByText("Details", { exact: true }).click()
  await expect(
    page.getByText("legacy property pass failed: platform command failed with exit status: 1", {
      exact: true,
    }),
  ).toBeVisible()
  await page.getByRole("button", { name: "Stop spoofing", exact: true }).click()
  await expect(page.locator("#faker-config")).toHaveValue(draft)
})

for (const theme of ["light", "dark"] as const) {
  test(`startup failure keeps headings, fields and touch controls aligned in ${theme}`, async ({
    page,
    open,
  }) => {
    await page.setViewportSize({ width: 390, height: 1000 })
    await open("/device-faker", { locale: "en", theme, query: { "faker-startup-failed": "1" } })
    const headings = await page.locator("section > header h2").all()
    const field = await page.locator("#faker-config").boundingBox()
    for (const heading of headings) {
      const box = await heading.boundingBox()
      expect(Math.abs(box!.x - field!.x)).toBeLessThanOrEqual(1)
    }
    await page.getByText("Details", { exact: true }).click()
    const controls = page.locator("section").first().getByRole("button")
    for (const button of await controls.all()) {
      expect((await button.boundingBox())!.height).toBeGreaterThanOrEqual(48)
    }
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
    await page.screenshot({ path: `/tmp/duck-faker-startup-${theme}.png` })
  })
}

for (const prerequisite of [
  "faker-startup-pending",
  "faker-cleanup-pending",
  "faker-conflict",
  "faker-parent-disabled",
]) {
  test(`an armed engine does not claim injection while ${prerequisite}`, async ({ page, open }) => {
    await open("/device-faker", {
      locale: "en",
      query: { "faker-enabled": "1", [prerequisite]: "1" },
    })
    await expect(page.getByText("Enabled for new app processes", { exact: true })).toHaveCount(0)
    await expect(page.getByRole("button", { name: "Enable", exact: true })).toBeDisabled()
    await page.getByRole("button", { name: "Stop spoofing", exact: true }).click()
    await expect(page.getByText("Disabled for new app processes", { exact: true })).toBeVisible()
  })
}

test("preserves an oversized multilingual draft across navigation without allowing publication", async ({
  page,
  open,
}) => {
  await open("/device-faker", { locale: "en" })
  const draft = '[[apps]]\npackage = "com.example.app"\nmodel = "' + "设备".repeat(12000) + '"\n'
  await page.locator("#faker-config").fill(draft)
  await expect(page.locator("#faker-config")).toHaveAttribute("aria-invalid", "true")
  await expect(page.locator("#faker-config-size")).toBeVisible()
  await page.evaluate(() => {
    location.hash = "/system"
  })
  await expect(page.locator("#faker-config")).toHaveCount(0)
  await page.evaluate(() => {
    location.hash = "/device-faker"
  })
  await expect(page.locator("#faker-config")).toHaveValue(draft)
  await expect(
    page.getByRole("button", { name: "Validate and preview", exact: true }),
  ).toBeDisabled()
  await expect(page.getByRole("button", { name: "Apply configuration", exact: true })).toHaveCount(
    0,
  )
  await expect(page.getByRole("button", { name: "Stop spoofing", exact: true })).toBeEnabled()
})

test("an identical cached configuration uses the current saved revision", async ({
  page,
  open,
}) => {
  await page.addInitScript(() =>
    sessionStorage.setItem(
      "duck-toolbox/device-faker/draft",
      JSON.stringify({ content: "debug = false\n\n[templates]\n", revision: "old-revision" }),
    ),
  )
  await open("/device-faker", { locale: "en" })
  await expect(page.locator("#faker-config")).toHaveValue("debug = false\n\n[templates]\n")
  await expect(page.getByText("Saved state changed.", { exact: false })).toHaveCount(0)
})

for (const locale of ["en", "zh-CN", "ar", "de"]) {
  test(`lost global ownership stays readable and cannot be enabled in ${locale}`, async ({
    page,
    open,
  }) => {
    await page.setViewportSize({ width: 320, height: 900 })
    await open("/device-faker", {
      locale,
      query: { "faker-enabled": "1", "faker-orphaned-global": "1" },
    })
    await page.evaluate(() => {
      document.documentElement.style.fontSize = "200%"
    })
    await expect(page.getByRole("alert")).toHaveCount(1)
    await expect(page.getByRole("alert")).toBeVisible()
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true)
    if (locale === "en" || locale === "ar")
      await page.getByRole("alert").screenshot({ path: `/tmp/duck-global-recovery-${locale}.png` })
    const buttons = page.locator("section").first().getByRole("button")
    await expect(buttons.nth(0)).toBeDisabled()
    await expect(buttons.nth(1)).toBeEnabled()
    await buttons.nth(1).click()
    await expect(page.getByRole("alert")).toBeVisible()
  })
}
