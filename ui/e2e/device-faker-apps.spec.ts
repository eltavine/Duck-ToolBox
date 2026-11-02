import { readFileSync } from "node:fs"
import { expect, test } from "./fixtures"

function messages(locale: string) {
  return JSON.parse(
    readFileSync(
      new URL(`../src/features/device-faker/locales/${locale}.json`, import.meta.url),
      "utf8",
    ),
  ) as {
    addProfile: string
    packages: string
    append: string
    fields: Record<string, string>
    apps: Record<string, string>
  }
}

test("selects multiple apps across searches without broadening existing user keys or saving", async ({
  page,
  open,
}) => {
  const t = messages("en")
  await open("/device-faker", { locale: "en" })
  const draft = page.locator("#faker-config")
  const saved = await draft.inputValue()
  await page.getByText(t.addProfile, { exact: true }).click()
  const keys = page.getByLabel(t.packages, { exact: true })
  const manual = "com.tencent.mm@999, com.offline.app@0"
  await keys.fill(manual)
  await page.getByRole("button", { name: t.apps.title, exact: true }).click()
  const dialog = page.getByRole("dialog")
  const search = dialog.getByRole("searchbox")
  await search.fill("com.tencent.mm")
  await expect(dialog.getByRole("checkbox")).toBeChecked()
  await expect(dialog.getByRole("checkbox")).toBeDisabled()
  await search.fill("wallet")
  await dialog.getByText("Google Wallet", { exact: true }).click()
  await search.fill("paypal")
  await dialog.getByRole("checkbox").focus()
  await page.keyboard.press("Space")
  await search.fill("no-such-app")
  await expect(dialog.getByText(t.apps.noResults!, { exact: true })).toBeVisible()
  await expect(dialog.getByRole("status")).toHaveText("Selected: 2")
  await dialog.getByRole("button", { name: t.apps.add, exact: true }).click()
  await expect(dialog).toHaveCount(0)
  await expect(keys).toHaveValue(
    manual + "\ncom.google.android.apps.walletnfcrel\ncom.paypal.android.p2pmobile",
  )
  await expect(draft).toHaveValue(saved)
  await page.getByLabel(t.fields.model!, { exact: true }).fill("Phone")
  await page.getByRole("button", { name: t.append, exact: true }).click()
  await expect(draft).toHaveValue(/package = "com.tencent.mm@999"/)
  await expect(draft).not.toHaveValue(/package = "com.tencent.mm"/)
  await expect(page.getByText("Disabled for new app processes", { exact: true })).toBeVisible()
})

test("cancel discards pending selections and restores keyboard focus", async ({ page, open }) => {
  const t = messages("en")
  await open("/device-faker", { locale: "en" })
  await page.getByText(t.addProfile, { exact: true }).click()
  const keys = page.getByLabel(t.packages, { exact: true })
  await keys.fill("com.manual.app@999")
  const trigger = page.getByRole("button", { name: t.apps.title, exact: true })
  await trigger.click()
  const dialog = page.getByRole("dialog")
  await dialog.getByRole("searchbox").fill("paypal")
  await dialog.getByText("PayPal", { exact: true }).click()
  await page.keyboard.press("Escape")
  await expect(dialog).toHaveCount(0)
  await expect(trigger).toBeFocused()
  await expect(keys).toHaveValue("com.manual.app@999")
  await trigger.click()
  await expect(dialog.getByRole("searchbox")).toHaveValue("")
  await expect(dialog.getByRole("status")).toHaveText("Selected: 0")
  await expect(dialog.getByRole("button", { name: t.apps.add, exact: true })).toBeDisabled()
})

test("keeps manual entry available when the host returns no installed apps", async ({
  page,
  open,
}) => {
  const t = messages("en")
  await open("/device-faker", { locale: "en" })
  await page.evaluate(() => {
    const host = (window as unknown as { ksu: { listPackages: () => string } }).ksu
    host.listPackages = () => "[]"
  })
  await page.getByText(t.addProfile, { exact: true }).click()
  await page.getByRole("button", { name: t.apps.title, exact: true }).click()
  const dialog = page.getByRole("dialog")
  await expect(dialog.getByText(t.apps.noResults!, { exact: true })).toBeVisible()
  await expect(dialog.getByRole("button", { name: t.apps.add, exact: true })).toBeDisabled()
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click()
  await page.getByLabel(t.packages, { exact: true }).fill("com.manual.app@999")
  await expect(page.getByLabel(t.packages, { exact: true })).toHaveValue("com.manual.app@999")
})

for (const locale of ["en", "zh-CN", "ar", "de"]) {
  for (const theme of ["light", "dark"] as const) {
    test(`app picker remains aligned with large text in ${locale} ${theme}`, async ({
      page,
      open,
    }, testInfo) => {
      const t = messages(locale)
      await page.setViewportSize({ width: 320, height: 900 })
      await open("/device-faker", { locale, theme })
      await page.evaluate(() => {
        document.documentElement.style.fontSize = "200%"
      })
      await page.getByText(t.addProfile, { exact: true }).click()
      const keys = page.getByLabel(t.packages, { exact: true })
      const trigger = page.getByRole("button", { name: t.apps.title, exact: true })
      expect(
        Math.abs((await keys.boundingBox())!.x - (await trigger.boundingBox())!.x),
      ).toBeLessThanOrEqual(1)
      await trigger.click()
      const dialog = page.getByRole("dialog")
      const search = dialog.getByRole("searchbox")
      await expect(search).toBeFocused()
      await search.fill("paypal")
      const row = dialog.locator("label").filter({ hasText: "PayPal" })
      await row.click()
      await expect(row.getByRole("checkbox")).toBeChecked()
      expect((await row.boundingBox())!.height).toBeGreaterThanOrEqual(48)
      const name = row.locator('span[dir="auto"]')
      expect((await name.boundingBox())!.height).toBeLessThanOrEqual(
        await name.evaluate((node) => Number.parseFloat(getComputedStyle(node).lineHeight) + 1),
      )
      const searchBox = (await search.boundingBox())!
      const rowBox = (await row.boundingBox())!
      expect(Math.abs(searchBox.x - rowBox.x)).toBeLessThanOrEqual(1)
      expect(Math.abs(searchBox.width - rowBox.width)).toBeLessThanOrEqual(1)
      const checkboxBox = (await row.locator("span[aria-hidden]").last().boundingBox())!
      const iconBox = (await row.locator("span.relative").boundingBox())!
      expect(
        Math.abs(checkboxBox.y + checkboxBox.height / 2 - iconBox.y - iconBox.height / 2),
      ).toBeLessThanOrEqual(1)
      expect(await dialog.evaluate((node) => node.scrollWidth <= node.clientWidth)).toBe(true)
      const add = dialog.getByRole("button", { name: t.apps.add, exact: true })
      await add.scrollIntoViewIfNeeded()
      const buttonBox = (await add.boundingBox())!
      expect(buttonBox.height).toBeGreaterThanOrEqual(48)
      expect(buttonBox.x).toBeGreaterThanOrEqual(0)
      expect(buttonBox.x + buttonBox.width).toBeLessThanOrEqual(320)
      await dialog.screenshot({
        path: testInfo.outputPath("app-picker.png"),
        animations: "disabled",
      })
      await add.click()
      await expect(keys).toHaveValue("com.paypal.android.p2pmobile")
    })
  }
}

for (const theme of ["light", "dark"] as const) {
  test(`app picker has aligned controls on a phone in ${theme}`, async ({
    page,
    open,
  }, testInfo) => {
    const t = messages("zh-CN")
    await page.setViewportSize({ width: 390, height: 844 })
    await open("/device-faker", { locale: "zh-CN", theme })
    await page.getByText(t.addProfile, { exact: true }).click()
    await page.getByRole("button", { name: t.apps.title, exact: true }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByText("Google Wallet", { exact: true }).click()
    expect(await dialog.evaluate((node) => node.scrollHeight <= node.clientHeight + 1)).toBe(true)
    expect(await dialog.evaluate((node) => node.scrollTop)).toBe(0)
    const box = (await dialog.boundingBox())!
    const heading = (await dialog.getByRole("heading").boundingBox())!
    expect(heading.y).toBeGreaterThanOrEqual(box.y)
    const edges = await Promise.all([
      dialog.locator('[data-slot="dialog-header"]').boundingBox(),
      dialog.getByRole("searchbox").boundingBox(),
      dialog.getByRole("group").boundingBox(),
      dialog.locator('[data-slot="dialog-footer"]').boundingBox(),
    ])
    for (const box of edges) {
      expect(Math.abs(box!.x - edges[0]!.x)).toBeLessThanOrEqual(1)
      expect(Math.abs(box!.width - edges[0]!.width)).toBeLessThanOrEqual(1)
    }
    await dialog.screenshot({
      path: testInfo.outputPath("phone-picker.png"),
      animations: "disabled",
    })
  })
}

test("keeps a large installed-app list virtualized and searchable", async ({ page, open }) => {
  const t = messages("en")
  await open("/device-faker", { locale: "en" })
  await page.evaluate(() => {
    const host = (
      window as unknown as {
        ksu: { listPackages: () => string; getPackagesInfo: (names: string) => string }
      }
    ).ksu
    const names = Array.from(
      { length: 500 },
      (_, index) => `com.test.app${String(index).padStart(4, "0")}`,
    )
    host.listPackages = () => JSON.stringify(names)
    host.getPackagesInfo = (json) =>
      JSON.stringify(
        (JSON.parse(json) as string[]).map((packageName) => ({
          packageName,
          appLabel: `Test app ${packageName.slice(-4)}`,
          isSystem: false,
        })),
      )
  })
  await page.getByText(t.addProfile, { exact: true }).click()
  await page.getByRole("button", { name: t.apps.title, exact: true }).click()
  const dialog = page.getByRole("dialog")
  const list = dialog.getByRole("group")
  await expect(dialog.getByText("Test app 0000", { exact: true })).toBeVisible()
  expect(await list.locator("label").count()).toBeLessThan(40)
  await list.evaluate((node) => {
    node.scrollTop = node.scrollHeight
  })
  await dialog.getByText("Test app 0499", { exact: true }).click()
  await dialog.getByRole("searchbox").fill("Test app 0020")
  await dialog.getByText("Test app 0020", { exact: true }).click()
  await expect(dialog.getByRole("status")).toHaveText("Selected: 2")
  await dialog.getByRole("button", { name: t.apps.add, exact: true }).click()
  await expect(page.getByLabel(t.packages, { exact: true })).toHaveValue(
    "com.test.app0499\ncom.test.app0020",
  )
})
