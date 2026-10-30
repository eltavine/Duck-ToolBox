import { expect, test } from "./fixtures"

test("changing the device-file path invalidates an in-flight import preview", async ({
  page,
  open,
}) => {
  await open("/play-integrity/config", { query: { latency: "400" } })
  await page.getByRole("button", { name: "Import, export and raw files", exact: true }).click()
  const source = page.getByRole("textbox", { name: "Import device file", exact: true })
  await expect(source).toHaveValue("/data/adb/play-integrity/custom.pif.json")
  await source.fill("/data/local/tmp/first.prop")
  const load = page.getByRole("button", { name: "Import device file", exact: true })
  await load.click()
  await expect(load).toBeDisabled()
  await source.fill("/data/local/tmp/second.prop")
  await expect(load).toBeEnabled()
  await expect(page.getByRole("button", { name: "Apply configuration", exact: true })).toHaveCount(
    0,
  )
  await expect(page.getByLabel("Configuration", { exact: true })).toHaveValue(/MODEL=Pixel 9 Pro/)
})

for (const directory of ["/data/adb/play-integrity", "/data/adb/pi-fixture"]) {
  test(`browses the engine data directory reported as ${directory}`, async ({ page, open }) => {
    await open("/play-integrity/config", {
      query: { "pif-data-dir": directory, files: "custom.pif.json" },
    })
    await page.getByRole("button", { name: "Import, export and raw files", exact: true }).click()
    const source = page.getByRole("textbox", { name: "Import device file", exact: true })
    await expect(source).toHaveValue(`${directory}/custom.pif.json`)
    await page.getByRole("button", { name: "Browse", exact: true }).click()
    const dialog = page.getByRole("dialog")
    await expect(dialog.getByRole("button", { name: directory.split("/").at(-1)! })).toBeVisible()
    await dialog.getByRole("button", { name: /custom\.pif\.json/ }).click()
    await expect(dialog).toBeHidden()
    await expect(source).toHaveValue(`${directory}/custom.pif.json`)
    await expect(page.getByRole("cell", { name: "Imported", exact: true })).toBeVisible()
    await page.getByRole("button", { name: "Dismiss preview", exact: true }).click()
    await page.getByRole("button", { name: "Edit fields", exact: true }).click()
    await expect(page.getByLabel("Device model · MODEL", { exact: true })).toHaveValue(
      "Pixel 9 Pro",
    )
  })
}

test("can leave a missing import directory through its parent and root breadcrumbs", async ({
  page,
  open,
}) => {
  await open("/play-integrity/config", {
    query: { "missing-directory": "/data/adb/modules/playintegrityfix" },
  })
  await page.getByRole("button", { name: "Import, export and raw files", exact: true }).click()
  const source = page.getByRole("textbox", { name: "Import device file", exact: true })
  const legacy = "/data/adb/modules/playintegrityfix/custom.pif.prop"
  await source.fill(legacy)
  await page.evaluate(() => {
    const scope = window as unknown as Record<string, unknown>
    const host = scope.ksu as { exec: (shell: string, options: string, callback: string) => void }
    const original = host.exec
    scope.fileCommands = []
    host.exec = (shell, options, callback) => {
      if (shell.includes("'system' 'files'")) (scope.fileCommands as string[]).push(shell)
      original(shell, options, callback)
    }
  })
  await page.getByRole("button", { name: "Browse", exact: true }).click()
  const dialog = page.getByRole("dialog")
  await expect(dialog.getByText(/No such file or directory/)).toBeVisible()
  await expect(dialog.getByRole("button", { name: "System file picker" })).toBeEnabled()
  await dialog.getByRole("button", { name: "Up one level", exact: true }).click()
  await expect(dialog.getByText(/No such file or directory/)).toHaveCount(0)
  await expect(dialog.getByRole("button", { name: /notes\.txt/ })).toBeVisible()
  await dialog.getByRole("button", { name: "data", exact: true }).click()
  await expect(dialog.getByRole("button", { name: "adb", exact: true })).toHaveCount(0)
  await dialog.getByRole("button", { name: "/", exact: true }).click()
  await expect(dialog.getByRole("button", { name: "Up one level", exact: true })).toHaveCount(0)
  await expect(dialog.getByRole("button", { name: /notes\.txt/ })).toBeVisible()
  const commands = await page.evaluate(
    () => (window as unknown as { fileCommands: string[] }).fileCommands,
  )
  expect(commands).toEqual(
    expect.arrayContaining([
      expect.stringContaining("'--path' '/data/adb/modules/playintegrityfix'"),
      expect.stringContaining("'--path' '/data/adb/modules'"),
      expect.stringContaining("'--path' '/data'"),
      expect.stringContaining("'--path' '/'"),
    ]),
  )
  expect(commands.some((command) => command.includes("'--path' '//"))).toBe(false)
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click()
  await expect(source).toHaveValue(legacy)
  await page.getByRole("button", { name: "Browse", exact: true }).click()
  await expect(dialog.getByText(/No such file or directory/)).toBeVisible()
  await dialog.getByRole("button", { name: "data", exact: true }).click()
  await expect(dialog.getByRole("button", { name: /notes\.txt/ })).toBeVisible()
})

test("can import from the system picker while the device directory is missing", async ({
  page,
  open,
}) => {
  await open("/play-integrity/config", {
    query: { "missing-directory": "/data/adb/modules/playintegrityfix" },
  })
  await page.getByRole("button", { name: "Import, export and raw files", exact: true }).click()
  await page
    .getByRole("textbox", { name: "Import device file", exact: true })
    .fill("/data/adb/modules/playintegrityfix/custom.pif.prop")
  await page.getByRole("button", { name: "Browse", exact: true }).click()
  const dialog = page.getByRole("dialog")
  await expect(dialog.getByText(/No such file or directory/)).toBeVisible()
  await dialog.locator('input[type="file"]').setInputFiles({
    name: "custom.pif.prop",
    mimeType: "text/plain",
    buffer: Buffer.from("MODEL=System file\nEXTRA=keep"),
  })
  await expect(dialog).toBeHidden()
  await expect(page.getByRole("cell", { name: "System file", exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Apply configuration", exact: true })).toBeEnabled()
  await page.reload()
  await expect(page.getByLabel("Device model · MODEL", { exact: true })).toHaveValue("Pixel 9 Pro")
})
