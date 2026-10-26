import { expect, test } from "./fixtures"

test("browses modules and parameters and refuses busy module removal", async ({ page, open }) => {
  await open("/kernel-modules")
  await expect(page.getByText("6.6.30-android15", { exact: true })).toBeVisible()
  await expect(
    page.locator('[data-module="cfg80211"]').getByRole("button", { name: /^Unload / }),
  ).toBeDisabled()
  await expect(
    page.locator('[data-module="binder"]').getByRole("button", { name: /^Unload / }),
  ).toHaveCount(0)
  await page.locator('[data-module="usb_f_mass_storage"]').getByRole("button").first().click()
  await expect(page.getByRole("dialog")).toBeVisible()
  await expect(page.getByText("Parameters (read only)", { exact: true })).toBeVisible()
  await expect(page.getByText("debug", { exact: true })).toBeVisible()
  await page.keyboard.press("Escape")
  await page.getByRole("textbox", { name: "Search modules", exact: true }).fill("cfg80211")
  await expect(page.locator("[data-module]")).toHaveCount(1)
})

test("confirms removal and refreshes the runtime inventory", async ({ page, open }) => {
  await open("/kernel-modules")
  await page
    .locator('[data-module="usb_f_mass_storage"]')
    .getByRole("button", { name: /^Unload / })
    .click()
  await expect(page.getByRole("alertdialog")).toBeVisible()
  await page.getByRole("alertdialog").getByRole("button", { name: "Cancel", exact: true }).click()
  await expect(
    page.getByRole("button", { name: "Unload usb_f_mass_storage", exact: true }),
  ).toBeFocused()
  await expect(page.locator('[data-module="usb_f_mass_storage"]')).toBeVisible()
  await page
    .locator('[data-module="usb_f_mass_storage"]')
    .getByRole("button", { name: /^Unload / })
    .click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Confirm", exact: true }).click()
  await expect(page.locator('[data-module="usb_f_mass_storage"]')).toHaveCount(0)
  await expect(page.getByRole("tab", { name: "Runtime", exact: true })).toBeFocused()
})

test("inspects a module, loads it after confirmation and clears stale inspection", async ({
  page,
  open,
}) => {
  await open("/kernel-modules")
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page.getByRole("button", { name: "/vendor/lib/modules/sample.ko", exact: false }).click()
  await expect(page.getByText("Kernel release matches.", { exact: true })).toBeVisible()
  await page
    .getByRole("textbox", { name: "Load parameters (optional)", exact: true })
    .fill("debug=1")
  await page.getByRole("tab", { name: "Runtime", exact: true }).click()
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page.getByRole("button", { name: "Load module", exact: true }).click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Confirm", exact: true }).click()
  await expect(page.getByRole("button", { name: "Load module", exact: true })).toBeDisabled()
  await page.getByRole("tab", { name: "Runtime", exact: true }).click()
  await expect(page.locator('[data-module="sample"]')).toBeVisible()
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page
    .getByRole("textbox", { name: "Device .ko path", exact: true })
    .fill("/data/local/tmp/other.ko")
  await expect(page.getByRole("button", { name: "Load module", exact: true })).toHaveCount(0)
})

test("keeps file inspection available when kernel operations are disabled", async ({
  page,
  open,
}) => {
  await open("/kernel-modules", { query: { kernelModules: "disabled" } })
  await expect(
    page.getByText("The kernel has disabled module loading and unloading.", { exact: true }),
  ).toBeVisible()
  await expect(
    page.locator('[data-module="usb_f_mass_storage"]').getByRole("button", { name: /^Unload / }),
  ).toBeDisabled()
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page
    .getByRole("textbox", { name: "Device .ko path", exact: true })
    .fill("/data/local/tmp/mismatch.ko")
  await page.getByRole("button", { name: "Inspect file", exact: true }).click()
  await expect(page.getByText("Kernel release does not match.", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Load module", exact: true })).toBeDisabled()
})

test("shows kernel rejection without adding the module", async ({ page, open }) => {
  await open("/kernel-modules", { query: { kernelModules: "reject" } })
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page
    .getByRole("textbox", { name: "Device .ko path", exact: true })
    .fill("/data/local/tmp/sample.ko")
  await page.getByRole("button", { name: "Inspect file", exact: true }).click()
  await page.getByRole("button", { name: "Load module", exact: true }).click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Confirm", exact: true }).click()
  await expect(page.getByText("The kernel rejected the operation.", { exact: true })).toBeVisible()
  await page.getByRole("tab", { name: "Runtime", exact: true }).click()
  await expect(page.locator('[data-module="sample"]')).toHaveCount(0)
})

test("treats release differences as advisory and shows the exact load request", async ({
  page,
  open,
}) => {
  await open("/kernel-modules")
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page
    .getByRole("textbox", { name: "Device .ko path", exact: true })
    .fill("/data/local/tmp/mismatch.ko")
  await page.getByRole("button", { name: "Inspect file", exact: true }).click()
  await expect(page.getByText("Kernel release does not match.", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Load module", exact: true })).toBeEnabled()
  await page
    .getByRole("textbox", { name: "Load parameters (optional)", exact: true })
    .fill("debug=1")
  await page.getByRole("button", { name: "Load module", exact: true }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog.getByText("/data/local/tmp/mismatch.ko", { exact: true })).toBeVisible()
  await expect(dialog.getByText("debug=1", { exact: true })).toBeVisible()
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click()
})

test("announces a pending operation and keeps draft parameters across tabs", async ({
  page,
  open,
}) => {
  await open("/kernel-modules", { query: { latency: "200" } })
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page
    .getByRole("textbox", { name: "Device .ko path", exact: true })
    .fill("/data/local/tmp/sample.ko")
  await page.getByRole("button", { name: "Inspect file", exact: true }).click()
  const parameters = page.getByRole("textbox", { name: "Load parameters (optional)", exact: true })
  await expect(parameters).toBeVisible()
  await parameters.fill("debug=1")
  await page.getByRole("tab", { name: "Runtime", exact: true }).click()
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await expect(parameters).toHaveValue("debug=1")
  await page.getByRole("button", { name: "Load module", exact: true }).click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Confirm", exact: true }).click()
  await expect(page.getByTestId("kernel-operation-pending")).toBeVisible()
  await expect(page.getByRole("button", { name: "Load module", exact: true })).toBeDisabled()
  await expect(page.getByTestId("kernel-operation-pending")).toHaveCount(0)
})

test("supports keyboard focus and reachable controls at 200 percent text", async ({
  page,
  open,
}) => {
  await page.setViewportSize({ width: 320, height: 568 })
  await open("/kernel-modules")
  await page.evaluate(() => {
    document.documentElement.style.fontSize = "32px"
  })
  const details = page.getByRole("button", {
    name: "View details of usb_f_mass_storage",
    exact: true,
  })
  await details.focus()
  await page.keyboard.press("Enter")
  await expect(page.getByRole("dialog")).toBeVisible()
  await page.keyboard.press("Escape")
  await expect(details).toBeFocused()
  for (const control of [
    details,
    page.getByRole("button", { name: "Unload usb_f_mass_storage", exact: true }),
  ]) {
    const bounds = await control.boundingBox()
    expect(bounds!.width).toBeGreaterThanOrEqual(48)
    expect(bounds!.height).toBeGreaterThanOrEqual(48)
  }
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
    ),
  ).toBe(true)
})

test("explains why an ARM32 module cannot be loaded by the ARM64 manager", async ({
  page,
  open,
}) => {
  await open("/kernel-modules")
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page
    .getByRole("textbox", { name: "Device .ko path", exact: true })
    .fill("/data/local/tmp/arm32.ko")
  await page.getByRole("button", { name: "Inspect file", exact: true }).click()
  await expect(
    page.getByText("Loading requires a little-endian ARM64 ELF64 module.", { exact: true }),
  ).toBeVisible()
  await expect(page.getByRole("button", { name: "Load module", exact: true })).toBeDisabled()
})

for (const locale of ["en", "zh-CN", "ar"] as const) {
  test(`preserves technical direction, alignment and modal bounds in ${locale}`, async ({
    page,
    open,
  }) => {
    await page.setViewportSize({ width: 320, height: 568 })
    await open("/kernel-modules", { locale })
    const row = page.locator('[data-module="usb_f_mass_storage"]')
    await expect(row).toBeVisible()
    await page.evaluate(() => window.scrollTo(0, 0))
    const alignment = await row
      .getByRole("button")
      .first()
      .evaluate((button) => {
        const name = button.querySelector("bdi")!
        const range = document.createRange()
        range.selectNodeContents(name)
        const text = range.getBoundingClientRect()
        const box = button.getBoundingClientRect()
        return {
          direction: getComputedStyle(button).direction,
          left: text.left - box.left,
          right: box.right - text.right,
        }
      })
    expect(locale === "ar" ? alignment.right : alignment.left).toBeLessThan(2)
    await page.screenshot({
      path: `test-results/screenshots/kernel-review-${locale}-runtime.png`,
      fullPage: true,
      animations: "disabled",
    })
    await page.getByRole("tab").nth(1).click()
    const input = page.locator("#kernel-module-path")
    await input.fill("/data/local/tmp/sample.ko")
    expect(await input.getAttribute("dir")).toBe("ltr")
    const labels = {
      en: { inspect: "Inspect file", load: "Load module" },
      "zh-CN": { inspect: "解析文件", load: "加载模块" },
      ar: { inspect: "فحص الملف", load: "تحميل الوحدة" },
    }[locale]
    await page.getByRole("button", { name: labels.inspect, exact: true }).click()
    const load = page.getByRole("button", { name: labels.load, exact: true })
    await expect(load).toBeVisible()
    await page.evaluate(() => window.scrollTo(0, 0))
    await page.screenshot({
      path: `test-results/screenshots/kernel-review-${locale}-file.png`,
      fullPage: true,
      animations: "disabled",
    })
    await load.click()
    const dialog = page.getByRole("alertdialog")
    await expect(dialog).toBeVisible()
    await dialog.evaluate(async (element) => {
      await Promise.all(
        element.getAnimations().map((animation) => animation.finished.catch(() => undefined)),
      )
    })
    const bounds = await dialog.boundingBox()
    expect(bounds!.x).toBeGreaterThanOrEqual(0)
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(320)
    expect(bounds!.y).toBeGreaterThanOrEqual(0)
    expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(568)
    await page.screenshot({
      path: `test-results/screenshots/kernel-review-${locale}-confirm.png`,
      fullPage: false,
      animations: "disabled",
    })
  })
}

for (const theme of ["light", "dark"] as const) {
  test(`destructive labels and confirmation remain readable in ${theme}`, async ({
    page,
    open,
  }) => {
    await open("/kernel-modules", { theme })
    const unload = page.getByRole("button", { name: "Unload usb_f_mass_storage", exact: true })
    await unload.click()
    const confirm = page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Confirm", exact: true })
    const ratios = await confirm.evaluate((button) => {
      const canvas = document.createElement("canvas")
      canvas.width = canvas.height = 1
      const context = canvas.getContext("2d")!
      const root = getComputedStyle(document.documentElement)
      function luminance(colors: string[]) {
        context.clearRect(0, 0, 1, 1)
        for (const color of colors) {
          context.fillStyle = color
          context.fillRect(0, 0, 1, 1)
        }
        const [r, g, b] = [...context.getImageData(0, 0, 1, 1).data].slice(0, 3).map((v) => {
          const c = v / 255
          return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
        })
        return r! * 0.2126 + g! * 0.7152 + b! * 0.0722
      }
      const ratio = (a: number, b: number) => (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05)
      const red = luminance([root.getPropertyValue("--destructive")])
      const style = getComputedStyle(button)
      return {
        text: ratio(red, luminance([root.getPropertyValue("--background")])),
        row: ratio(red, luminance([root.getPropertyValue("--card")])),
        confirm: ratio(
          luminance([style.color]),
          luminance([root.getPropertyValue("--popover"), style.backgroundColor]),
        ),
      }
    })
    for (const [surface, ratio] of Object.entries(ratios))
      expect(ratio, surface).toBeGreaterThanOrEqual(4.5)
  })
}

test("inspects from the keyboard and expands metadata only on demand", async ({ page, open }) => {
  await open("/kernel-modules")
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page.locator("#kernel-module-path").fill("/data/local/tmp/sample.ko")
  await page.locator("#kernel-module-path").press("Enter")
  await expect(page.getByText("Kernel release matches.", { exact: true })).toBeVisible()
  await expect(page.getByText("Example kernel module", { exact: true })).toHaveCount(0)
  await page.locator("summary").click()
  await expect(page.getByText("Example kernel module", { exact: true })).toBeVisible()
})

test("explains the ARM64 module name limit before loading", async ({ page, open }) => {
  await open("/kernel-modules")
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page.locator("#kernel-module-path").fill("/data/local/tmp/long-name.ko")
  await page.locator("#kernel-module-path").press("Enter")
  await expect(
    page.getByText("ARM64 module names must fit in 55 bytes.", { exact: true }),
  ).toBeVisible()
  await expect(page.getByRole("button", { name: "Load module", exact: true })).toBeDisabled()
})
