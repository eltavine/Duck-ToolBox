import { expect, test } from "./fixtures"

test("opens language spoofing from Miscellaneous and saves without enabling", async ({
  page,
  open,
}) => {
  await open("/")
  await page.getByRole("button", { name: /Miscellaneous/ }).click()
  await page.getByRole("button", { name: /Language spoofing/ }).click()
  await expect(page.getByRole("button", { name: "Enable", exact: true })).toBeDisabled()
  await page.getByRole("button", { name: "Add", exact: true }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByRole("button", { name: "Choose installed app" }).click()
  await dialog.getByRole("searchbox").fill("com.tencent.mm")
  await dialog.getByRole("button", { name: /com.tencent.mm/ }).click()
  await dialog.getByLabel("Language", { exact: true }).fill("en-US")
  await dialog.getByRole("button", { name: "Done", exact: true }).click()
  await expect(page.getByText("com.tencent.mm", { exact: true })).toBeVisible()
  await page.getByRole("button", { name: "Save", exact: true }).click()
  await expect(page.getByRole("status").filter({ hasText: /^Disabled$/ })).toBeVisible()
  await page.getByRole("button", { name: "Enable", exact: true }).click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Confirm", exact: true }).click()
  await expect(
    page.getByRole("status").filter({ hasText: /^Enabled for new app processes$/ }),
  ).toBeVisible()
  await page.getByRole("button", { name: "Disable", exact: true }).click()
  await expect(page.getByRole("status").filter({ hasText: /^Disabled$/ })).toBeVisible()
  await expect(page.getByText("com.tencent.mm", { exact: true })).toBeVisible()
})

test("keeps stop available with corrupt rules", async ({ page, open }) => {
  await open("/misc/language", { query: { language: "corrupt" } })
  await expect(
    page.getByRole("alert").filter({ hasText: /configuration is invalid/ }),
  ).toBeVisible()
  await expect(page.getByRole("button", { name: "Enable", exact: true })).toBeDisabled()
  await page.getByRole("button", { name: "Disable", exact: true }).click()
  await expect(page.getByRole("button", { name: "Disable", exact: true })).toBeEnabled()
})

for (const variant of [
  { locale: "zh-CN", width: 320, height: 568, scale: 2, theme: "dark" },
  { locale: "en", width: 412, height: 839, scale: 1, theme: "light" },
  { locale: "ar", width: 320, height: 568, scale: 1, theme: "light" },
  { locale: "de", width: 568, height: 320, scale: 1, theme: "dark" },
] as const) {
  test(`language editor fits ${variant.locale} ${variant.theme}`, async ({ page, open }) => {
    await page.setViewportSize({ width: variant.width, height: variant.height })
    await open("/misc/language", {
      locale: variant.locale,
      theme: variant.theme,
      query: { language: "rules" },
    })
    await page.evaluate((scale) => {
      document.documentElement.style.fontSize = `${16 * scale}px`
    }, variant.scale)
    await expect(page.getByText("com.tencent.mm", { exact: true })).toBeVisible()
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1,
      ),
    ).toBe(true)
    await page.screenshot({
      path: `test-results/misc-language-page-${variant.locale}-${variant.theme}.png`,
      animations: "disabled",
    })
    await page
      .getByRole("button", { name: /com.tencent.mm/ })
      .first()
      .click()
    const dialog = page.getByRole("dialog")
    await expect(dialog).toBeVisible()
    const box = await dialog.boundingBox()
    expect(box!.x).toBeGreaterThanOrEqual(-1)
    expect(box!.y).toBeGreaterThanOrEqual(-1)
    expect(box!.x + box!.width).toBeLessThanOrEqual(variant.width + 1)
    expect(box!.y + box!.height).toBeLessThanOrEqual(variant.height + 1)
    expect(await dialog.evaluate((element) => element.scrollWidth <= element.clientWidth + 1)).toBe(
      true,
    )
    await page.screenshot({
      path: `test-results/misc-language-${variant.locale}-${variant.theme}.png`,
      fullPage: false,
      animations: "disabled",
    })
  })
}

test("keeps edits when a discard confirmation is cancelled", async ({ page, open }) => {
  await open("/misc/language", { query: { language: "rules" } })
  await page.getByRole("button", { name: "Edit com.tencent.mm", exact: true }).click()
  await page.getByRole("dialog").getByLabel("Language", { exact: true }).fill("ja-JP")
  await page.getByRole("dialog").getByRole("button", { name: "Done", exact: true }).click()
  await expect(page.getByRole("button", { name: "Enable", exact: true })).toBeDisabled()
  await page.getByRole("button", { name: "Back", exact: true }).click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Cancel", exact: true }).click()
  await expect(page.getByText("Unsaved changes", { exact: true })).toBeVisible()
  await expect(page.getByText("com.tencent.mm", { exact: true })).toBeVisible()
})

test("captures the Chinese language page", async ({ page, open }) => {
  await page.setViewportSize({ width: 412, height: 839 })
  await open("/misc/language", { locale: "zh-CN", theme: "dark", query: { language: "rules" } })
  await expect(page.getByText("com.tencent.mm", { exact: true })).toBeVisible()
  await page.screenshot({
    path: "test-results/misc-language-zh-CN-preview.png",
    animations: "disabled",
    fullPage: true,
  })
})
