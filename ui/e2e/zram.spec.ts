import { expect, test } from "./fixtures"

test("applies only after confirmation, restores original parameters and compares memory", async ({
  page,
  open,
}) => {
  await open("/zram")
  await expect(page.getByText("Actual memory allocated", { exact: true })).toBeVisible()
  const size = page.getByRole("textbox", { name: "Capacity (MiB)", exact: true })
  await size.fill("2048")
  await page.getByRole("button", { name: "Apply now", exact: true }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog).toBeVisible()
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click()
  await expect(size).toHaveValue("2048")
  await expect(page.getByRole("button", { name: "Apply now", exact: true })).toBeFocused()
  await page.getByRole("button", { name: "Apply now", exact: true }).click()
  await dialog.getByRole("button", { name: "Apply now", exact: true }).click()
  await expect(page.getByText("Memory after reset", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Restore original", exact: true })).toBeEnabled()
  await page.getByRole("button", { name: "Restore original", exact: true }).click()
  await dialog.getByRole("button", { name: "Restore original", exact: true }).click()
  await expect(page.getByRole("alertdialog")).toHaveCount(0)
  await expect(page.getByTestId("zram-page")).toBeFocused()
})
test("saves boot application separately from runtime changes and can remove it", async ({
  page,
  open,
}) => {
  await open("/zram")
  await page.getByRole("switch", { name: "Apply at boot", exact: true }).click()
  await page.getByRole("button", { name: "Save plan", exact: true }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog.getByText("Enabled", { exact: true })).toBeVisible()
  await dialog.getByRole("button", { name: "Save plan", exact: true }).click()
  await expect(page.getByText("Saved: boot application enabled", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Restore original", exact: true })).toBeDisabled()
  await page.getByRole("button", { name: "Remove saved plan", exact: true }).click()
  await expect(page.getByRole("button", { name: "Remove saved plan", exact: true })).toHaveCount(0)
})
for (const scenario of ["mmd", "writeback", "unsupported", "read-only"]) {
  test(`blocks mutations for ${scenario} while retaining statistics`, async ({ page, open }) => {
    await open("/zram", { query: { zram: scenario } })
    await expect(page.getByText("Actual memory allocated", { exact: true })).toBeVisible()
    await expect(page.getByRole("button", { name: "Apply now", exact: true })).toBeDisabled()
    await expect(page.getByRole("button", { name: "Save plan", exact: true })).toBeDisabled()
  })
}
for (const scenario of ["reject", "timeout"]) {
  test(`keeps ${scenario} failure visible and retains the draft`, async ({ page, open }) => {
    await open("/zram", { query: { zram: scenario } })
    await page.getByRole("textbox", { name: "Capacity (MiB)", exact: true }).fill("1024")
    await page.getByRole("button", { name: "Apply now", exact: true }).click()
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Apply now", exact: true })
      .click()
    await expect(page.getByRole("alert")).toContainText(
      scenario === "timeout" ? "Timed out; outcome unknown" : "Available memory is insufficient",
    )
    await expect(page.getByRole("textbox", { name: "Capacity (MiB)", exact: true })).toHaveValue(
      "1024",
    )
  })
}
test("validates capacity and automatic priority before confirming", async ({ page, open }) => {
  await open("/zram")
  const size = page.getByRole("textbox", { name: "Capacity (MiB)", exact: true })
  await size.fill("99999")
  await expect(page.getByRole("button", { name: "Apply now", exact: true })).toBeDisabled()
  await expect(size).toHaveAttribute("aria-invalid", "true")
  await size.fill("0")
  await page.getByRole("textbox", { name: "Swap priority", exact: true }).fill("-1")
  await expect(page.getByRole("button", { name: "Apply now", exact: true })).toBeEnabled()
})
for (const [locale, theme, label, autoLabel] of [
  ["en", "light", "Apply now", "Automatic priority"],
  ["zh-CN", "dark", "立即应用", "自动优先级"],
  ["ar", "light", "تطبيق الآن", "أولوية تلقائية"],
] as const) {
  test(`aligns ${locale} ${theme} at 320px and 200% text`, async ({ page, open }) => {
    await page.setViewportSize({ width: 320, height: 568 })
    await open("/zram", { locale, theme })
    await expect(page.getByRole("button", { name: label, exact: true })).toBeVisible()
    await page.screenshot({
      path: `test-results/screenshots/zram-${locale}-${theme}-normal.png`,
      fullPage: true,
      animations: "disabled",
    })
    await page.addStyleTag({ content: "html { font-size: 200%; }" })
    await expect(page.getByRole("button", { name: label, exact: true })).toBeVisible()
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
    const toggleGeometry = await page
      .getByRole("switch", { name: autoLabel, exact: true })
      .evaluate((el) => {
        const text = el.parentElement!.querySelector(":scope > span")!.getBoundingClientRect()
        const control = el.getBoundingClientRect()
        return { textBottom: text.bottom, controlTop: control.top }
      })
    expect(toggleGeometry.controlTop).toBeGreaterThanOrEqual(toggleGeometry.textBottom)
    await page.evaluate(() => scrollTo(0, 0))
    await page.screenshot({
      path: `test-results/screenshots/zram-${locale}-${theme}.png`,
      fullPage: true,
      animations: "disabled",
    })
    await page.getByRole("button", { name: label, exact: true }).click()
    const dialog = page.getByRole("alertdialog")
    await expect(dialog).toBeVisible()
    await expect(dialog.getByRole("button", { name: label, exact: true })).toBeVisible()
    await dialog.getByRole("button", { name: label, exact: true }).scrollIntoViewIfNeeded()
    await expect(dialog.getByRole("button", { name: label, exact: true })).toBeInViewport()
    await page.screenshot({
      path: `test-results/screenshots/zram-${locale}-confirm.png`,
      animations: "disabled",
    })
    expect(await dialog.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true)
  })
}

test("keeps inspection available while another operation holds the lock", async ({
  page,
  open,
}) => {
  await open("/zram", { query: { zram: "busy" } })
  await expect(page.getByRole("status")).toContainText("Another ZRAM operation is running.")
  await expect(page.getByRole("button", { name: "Remove saved plan", exact: true })).toBeDisabled()
  await expect(page.getByRole("button", { name: "Apply now", exact: true })).toBeDisabled()
  await expect(page.getByRole("button", { name: "Refresh", exact: true })).toBeEnabled()
})
test("does not silently round an existing capacity or mark unrelated fields invalid", async ({
  page,
  open,
}) => {
  await open("/zram", { query: { zram: "fractional" } })
  const size = page.locator("#zram-size-zram0"),
    priority = page.locator("#zram-priority-zram0")
  await expect(size).toHaveValue("4096.00390625")
  await expect(size).toHaveAttribute("aria-invalid", "true")
  await expect(priority).toHaveAttribute("aria-invalid", "false")
  await expect(page.locator("#zram-size-error-zram0")).toBeVisible()
  await size.fill("4096")
  await priority.fill("-2")
  await expect(size).toHaveAttribute("aria-invalid", "false")
  await expect(priority).toHaveAttribute("aria-invalid", "true")
})
test("accepts Arabic digits and a localized minus sign without changing the requested values", async ({
  page,
  open,
}) => {
  await open("/zram", { locale: "ar" })
  await page.locator("#zram-size-zram0").fill("١٠٢٤")
  await page.locator("#zram-priority-zram0").fill("−١")
  await expect(page.getByRole("button", { name: "تطبيق الآن", exact: true })).toBeEnabled()
})
test("reports plan failures without claiming a kernel transition occurred", async ({
  page,
  open,
}) => {
  await open("/zram", { query: { zram: "save-failure" } })
  await page.getByRole("button", { name: "Save plan", exact: true }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Save plan", exact: true })
    .click()
  await expect(page.getByRole("alert")).toContainText("The saved plan state could not be confirmed")
  await expect(page.getByRole("alert")).not.toContainText("Some steps may already")
})

test("keeps field errors aligned with their controls on a wider screen", async ({ page, open }) => {
  await page.setViewportSize({ width: 800, height: 900 })
  await open("/zram")
  await page.locator("#zram-size-zram0").fill("99999")
  await page.locator("#zram-priority-zram0").fill("-2")
  const editor = page
    .locator("section")
    .filter({ has: page.getByRole("heading", { name: "Configuration", exact: true }) })
  await expect(editor.locator("#zram-priority-error-zram0")).toBeVisible()
  await editor.screenshot({
    path: "test-results/screenshots/zram-field-errors.png",
    animations: "disabled",
  })
  expect(await editor.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true)
})

for (const scenario of ["stable", "recreated"]) {
  test(`refreshing a ${scenario} device handles unsaved drafts correctly`, async ({
    page,
    open,
  }) => {
    await open("/zram", { query: { zram: scenario } })
    await page.locator("#zram-size-zram0").fill("1024")
    await page.getByRole("button", { name: "Refresh", exact: true }).click()
    await expect(page.getByRole("button", { name: "Refresh", exact: true })).toBeEnabled()
    await expect(page.locator("#zram-size-zram0")).toHaveValue(
      scenario === "recreated" ? "2048" : "1024",
    )
  })
}

test("offers automatic priority without typing a minus sign and retains the explicit value", async ({
  page,
  open,
}) => {
  await open("/zram")
  const priority = page.locator("#zram-priority-zram0")
  await priority.fill("500")
  const automatic = page.getByRole("switch", { name: "Automatic priority", exact: true })
  await automatic.click()
  await expect(priority).not.toBeEditable()
  await expect(automatic).toHaveAttribute("aria-checked", "true")
  await automatic.click()
  await expect(priority).toBeEditable()
  await expect(priority).toHaveValue("500")
})
test("rejects state that cannot be decoded accurately before enabling changes", async ({
  page,
  open,
}) => {
  await open("/zram", { query: { zram: "precision" } })
  await expect(
    page.getByText("Device data could not be decoded reliably. Refresh and inspect the details.", {
      exact: true,
    }),
  ).toBeVisible()
  await expect(page.getByRole("button", { name: "Apply now", exact: true })).toHaveCount(0)
})

test("allows removing a boot plan when its device is unavailable", async ({ page, open }) => {
  await open("/zram", { query: { zram: "orphan" } })
  await expect(
    page.getByRole("heading", { name: "Plans for unavailable devices", exact: true }),
  ).toBeVisible()
  await expect(page.getByRole("button", { name: "Apply now", exact: true })).toHaveCount(0)
  await page.getByRole("button", { name: "Remove saved plan zram0", exact: true }).click()
  await expect(
    page.getByRole("heading", { name: "Plans for unavailable devices", exact: true }),
  ).toHaveCount(0)
  await expect(page.getByTestId("zram-page")).toBeFocused()
})
