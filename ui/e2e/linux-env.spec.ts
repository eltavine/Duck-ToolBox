import { readFileSync, readdirSync } from "node:fs"
import { expect, test } from "./fixtures"
const locales = new URL("../src/features/linux-env/locales/", import.meta.url)

test("start, logs, stop and destructive confirmation retain the selected environment", async ({
  page,
  open,
}) => {
  await open("/linux-env", { locale: "en" })
  const root = page.getByTestId("linux-env-page")
  await root.getByRole("button", { name: "Start", exact: true }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog.getByText("debian", { exact: true })).toBeVisible()
  await dialog.getByRole("button", { name: "Start", exact: true }).click()
  await expect(root.getByText("Running", { exact: true })).toBeVisible()
  await root.getByRole("button", { name: "Read logs", exact: true }).click()
  await expect(root.locator("pre")).toContainText("Listening on 8080")
  await root.getByRole("button", { name: "Stop", exact: true }).click()
  await dialog.getByRole("button", { name: "Stop", exact: true }).click()
  await expect(root.getByText("Stopped", { exact: true })).toBeVisible()
  await root.getByRole("button", { name: "Delete environment", exact: true }).click()
  await expect(dialog).toContainText("Permanently delete")
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click()
  await expect(root.getByText("Debian", { exact: true })).toBeVisible()
  await root.getByRole("button", { name: "Delete environment", exact: true }).click()
  await dialog.getByRole("button", { name: "Delete environment", exact: true }).click()
  await expect(root.getByText("No environments imported.")).toBeVisible()
})
test("configuration validates DNS and services and previews the exact command", async ({
  page,
  open,
}) => {
  await open("/linux-env", { locale: "en" })
  const root = page.getByTestId("linux-env-page")
  await root.locator("summary").click()
  await page.locator("#linux-dns-debian").fill("not an IP")
  const save = root.getByRole("button", { name: "Save configuration", exact: true })
  await expect(save).toBeDisabled()
  await page.locator("#linux-dns-debian").fill("1.1.1.1\n2606:4700:4700::1111")
  await root.getByRole("button", { name: "Add service", exact: true }).click()
  await page.locator("#linux-command-debian-0").fill("exec python3 -m http.server 8080")
  await root.getByRole("switch", { name: "Start after Android boots", exact: true }).click()
  await save.click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog).toContainText("exec python3 -m http.server 8080")
  await expect(dialog).toContainText("Start after Android boots")
  await dialog.getByRole("button", { name: "Save configuration", exact: true }).click()
  await expect(page.locator("#linux-command-debian-0")).toHaveValue(
    "exec python3 -m http.server 8080",
  )
  await expect(root.getByText("Stopped", { exact: true })).toBeVisible()
  await expect(root.getByRole("button", { name: "Save configuration", exact: true })).toBeEnabled()
  await expect(root.getByRole("alert")).toHaveCount(0)
})
test("a changed environment refuses mutation", async ({ page, open }) => {
  await open("/linux-env", { locale: "en", query: { linuxEnv: "changed" } })
  await page.getByRole("button", { name: "Start", exact: true }).click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Start", exact: true }).click()
  await expect(page.getByTestId("linux-env-page").getByRole("alert")).toBeVisible()
  await expect(page.getByText("Stopped", { exact: true })).toBeVisible()
})
test("inspection is invalidated by path edits and import publishes progress", async ({
  page,
  open,
}) => {
  await open("/linux-env", { locale: "en", query: { linuxEnv: "empty" } })
  await page.locator("#linux-archive").fill("/sdcard/rootfs.tar.gz")
  await page.getByRole("button", { name: "Inspect archive", exact: true }).click()
  const start = page.getByRole("button", { name: "Import rootfs", exact: true })
  await expect(start).toBeEnabled()
  await page.locator("#linux-archive").fill("/sdcard/changed.tar.gz")
  await expect(start).toBeDisabled()
  await page.getByRole("button", { name: "Inspect archive", exact: true }).click()
  await expect(start).toBeEnabled()
  await start.click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog).toContainText("/sdcard/changed.tar.gz")
  await expect(dialog).toContainText("abababab")
  await dialog.getByRole("button", { name: "Import rootfs", exact: true }).click()
  await expect(
    page.getByTestId("linux-env-page").getByText("Imported", { exact: true }),
  ).toBeVisible({ timeout: 10000 })
  await expect(page.getByRole("button", { name: "Start", exact: true })).toBeEnabled()
})
test("read-only inspection remains available while mutations stay disabled", async ({
  page,
  open,
}) => {
  await open("/linux-env", { locale: "en", query: { linuxEnv: "readonly" } })
  await expect(page.getByRole("button", { name: "Start", exact: true })).toBeDisabled()
  await page.locator("#linux-archive").fill("/sdcard/rootfs.tar.gz")
  await page.getByRole("button", { name: "Inspect archive", exact: true }).click()
  await expect(page.getByText("ab".repeat(32), { exact: true })).toHaveCount(2)
  await expect(page.getByRole("button", { name: "Import rootfs", exact: true })).toBeDisabled()
})
test("running environments refresh automatically and preserve a draft", async ({ page, open }) => {
  await open("/linux-env", { locale: "en", query: { linuxEnv: "exited" } })
  const root = page.getByTestId("linux-env-page")
  await expect(root.getByText("Running", { exact: true })).toBeVisible()
  await root.locator("summary").click()
  await page.locator("#linux-config-name-debian").fill("Unsaved name")
  await expect(root.getByText("State unknown", { exact: true })).toBeVisible({ timeout: 10000 })
  await expect(page.locator("#linux-config-name-debian")).toHaveValue("Unsaved name")
  await expect(root.getByRole("button", { name: "Delete environment", exact: true })).toBeDisabled()
})
test("inspection keeps pending and error feedback next to its controls", async ({ page, open }) => {
  await open("/linux-env", { locale: "en", query: { latency: "500" } })
  await page.locator("#linux-archive").fill("/sdcard/missing.tar")
  await page.getByRole("button", { name: "Inspect archive", exact: true }).click()
  await expect(page.getByText("Calculating archive SHA-256…", { exact: true })).toBeVisible()
  await expect(page.getByTestId("linux-env-page").getByRole("alert")).toContainText(
    "The archive cannot be imported.",
  )
  await page.locator("#linux-archive").fill("/sdcard/rootfs.tar")
  await expect(page.getByTestId("linux-env-page").getByRole("alert")).toHaveCount(0)
})
test("cancelled imports and disabled settings are explicit", async ({ page, open }) => {
  await open("/linux-env", { locale: "en", query: { linuxEnv: "cancelled" } })
  const root = page.getByTestId("linux-env-page")
  await expect(root.getByText("Cancelled", { exact: true })).toBeVisible()
  await root.locator("summary").click()
  await root.getByRole("button", { name: "Save configuration", exact: true }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog.getByText("Disabled", { exact: true })).toBeVisible()
  await expect(
    dialog.getByText("Keep rootfs resolver configuration", { exact: true }),
  ).toBeVisible()
  await expect(dialog.getByText("No services configured.", { exact: true })).toBeVisible()
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click()
  await expect(root.getByRole("button", { name: "Save configuration", exact: true })).toBeFocused()
})
test("external configuration changes retain a draft until explicit reload", async ({
  page,
  open,
}) => {
  await open("/linux-env", { locale: "en", query: { linuxEnv: "config-change" } })
  const root = page.getByTestId("linux-env-page")
  await root.locator("summary").click()
  const name = page.locator("#linux-config-name-debian")
  await name.fill("My unsaved draft")
  await root.getByRole("button", { name: "Refresh", exact: true }).click()
  await expect(root.getByRole("alert")).toContainText(
    "The saved configuration changed elsewhere.",
    { timeout: 10000 },
  )
  await expect(name).toHaveValue("My unsaved draft")
  await expect(root.getByRole("button", { name: "Save configuration", exact: true })).toBeDisabled()
  await root.getByRole("button", { name: "Reload saved configuration", exact: true }).click()
  await expect(name).toHaveValue("Saved elsewhere")
  await expect(name).toBeFocused()
  await expect(root.getByRole("button", { name: "Save configuration", exact: true })).toBeEnabled()
})
test("service validation identifies the affected fields and command byte limit", async ({
  page,
  open,
}) => {
  await open("/linux-env", { locale: "en" })
  const root = page.getByTestId("linux-env-page")
  await root.locator("summary").click()
  for (let index = 0; index < 2; index++) {
    await root.getByRole("button", { name: "Add service", exact: true }).click()
    await page.locator(`#linux-service-debian-${index}`).fill("web")
    await page.locator(`#linux-command-debian-${index}`).fill("echo ready")
  }
  for (let index = 0; index < 2; index++)
    await expect(page.locator(`#linux-service-debian-${index}`)).toHaveAttribute(
      "aria-invalid",
      "true",
    )
  await page.locator("#linux-service-debian-1").fill("other")
  await page.locator("#linux-command-debian-1").fill("中".repeat(2731))
  await expect(page.locator("#linux-command-debian-1")).toHaveAttribute("aria-invalid", "true")
  await expect(page.locator("#linux-command-help-debian-1")).toHaveText(
    "1–8192 UTF-8 bytes; no NUL character.",
  )
  await expect(root.getByRole("button", { name: "Save configuration", exact: true })).toBeDisabled()
  await page.locator("#linux-command-debian-1").fill("echo ready")
  await expect(root.getByRole("button", { name: "Save configuration", exact: true })).toBeEnabled()
})
test("an empty log does not claim that environments are missing", async ({ page, open }) => {
  await open("/linux-env", { locale: "en", query: { linuxEnv: "empty-logs" } })
  const root = page.getByTestId("linux-env-page")
  await root.getByRole("button", { name: "Read logs", exact: true }).click()
  await expect(root.locator('pre[aria-label="Read logs"]')).toContainText(
    "No output has been recorded.",
  )
})
for (const file of readdirSync(locales).filter((f) => f.endsWith(".json"))) {
  const messages = JSON.parse(readFileSync(new URL(file, locales), "utf8")) as {
    overview: string
    configure: string
    save: string
    remove: string
  }
  test(`${file} keeps translated controls and confirmation within 320px`, async ({
    page,
    open,
  }) => {
    await page.setViewportSize({ width: 320, height: 568 })
    await open("/linux-env", { locale: file.replace(".json", "") })
    const root = page.getByTestId("linux-env-page")
    await expect(root.getByRole("heading", { name: messages.overview, exact: true })).toBeVisible()
    await root.locator("summary").click()
    await expect(root.getByRole("button", { name: messages.save, exact: true })).toBeVisible()
    await root.getByRole("button", { name: messages.remove, exact: true }).click()
    const dialog = page.getByRole("alertdialog")
    const button = dialog.getByRole("button", { name: messages.remove, exact: true })
    await button.scrollIntoViewIfNeeded()
    await expect(button).toBeInViewport()
    const width = await page.evaluate(() => ({
      scroll: document.documentElement.scrollWidth,
      width: innerWidth,
    }))
    expect(width.scroll).toBeLessThanOrEqual(width.width + 1)
  })
}
