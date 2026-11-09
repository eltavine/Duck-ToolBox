import { expect, test } from "@playwright/test"

for (const host of [
  "/data/adb/modules_update/duck-toolbox-lite",
  "/data/adb/modules/tricky_store",
]) {
  test(`Lite sends commands to its own backend when opened from ${host}`, async ({ page }) => {
    const wrapper = host.endsWith("tricky_store")
      ? "/data/adb/modules/duck-toolbox-lite/bin/duckctl.sh"
      : `${host}/bin/duckctl.sh`
    await page.addInitScript(
      ({ host, wrapper }) => {
        localStorage.setItem("duck-toolbox/locale", "en")
        const scope = window as unknown as Record<string, unknown>
        scope.ksu = {
          moduleInfo: () => JSON.stringify({ moduleDir: host }),
          exec(command: string, _options: string, callback: string) {
            const valid = command.startsWith(`'${wrapper}' `)
            if (valid) document.documentElement.setAttribute("data-lite-wrapper", wrapper)
            const contracts = { "tricky-store": 1, rkp: 3, "rkp-device": 7 }
            const features = Object.entries(contracts).map(([id, contract]) => ({
              id,
              summary: id,
              contract,
            }))
            const stdout = JSON.stringify({
              ok: valid,
              api: 1,
              command: "features",
              data: { api: 1, binary_version: "test", features },
              error: null,
              ts: 0,
            })
            setTimeout(() => (scope[callback] as (...args: unknown[]) => void)(0, stdout, ""))
          },
        }
      },
      { host, wrapper },
    )
    await page.goto("./")
    await expect(page.locator("html")).toHaveAttribute("data-lite-wrapper", wrapper)
    if (host.endsWith("tricky_store")) {
      await expect(page).toHaveURL(/#\/tricky-store$/)
      return
    }
    for (const title of [/Tricky Store/, /RKP Workbench/, /Device RKP Issuer/]) {
      await expect(page.getByRole("button", { name: title })).toBeEnabled()
    }
    await expect(
      page.getByRole("button", {
        name: /Play Integrity|Device Faker|Miscellaneous|Module & updates/,
      }),
    ).toHaveCount(0)
  })
}
