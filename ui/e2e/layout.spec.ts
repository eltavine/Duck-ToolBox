import { expect, test } from "./fixtures"
import type { Locator, Page } from "@playwright/test"

async function expectPanelFits(page: Page, panel: Locator) {
  await expect(panel).toBeVisible()
  const viewport = page.viewportSize()!
  await expect
    .poll(async () => {
      const bounds = await panel.boundingBox()
      return (
        !!bounds &&
        bounds.x >= -1 &&
        bounds.y >= -1 &&
        bounds.x + bounds.width <= viewport.width + 1 &&
        bounds.y + bounds.height <= viewport.height + 1
      )
    })
    .toBe(true)
  const dimensions = await panel.evaluate((element) => {
    const rect = element.getBoundingClientRect()
    return {
      top: rect.top,
      left: rect.left,
      right: rect.right,
      bottom: rect.bottom,
      scroll: element.scrollWidth,
      width: element.clientWidth,
    }
  })
  expect(dimensions.top).toBeGreaterThanOrEqual(-1)
  expect(dimensions.left).toBeGreaterThanOrEqual(-1)
  expect(dimensions.right).toBeLessThanOrEqual(viewport.width + 1)
  expect(dimensions.bottom).toBeLessThanOrEqual(viewport.height + 1)
  expect(dimensions.scroll).toBeLessThanOrEqual(dimensions.width + 1)
}

const routes = [
  "/",
  "/settings",
  "/system",
  "/system/activity",
  "/device-ids",
  "/rkp-device",
  "/tee-attestation",
  "/kernel-modules",
  "/linux-env",
  "/rkp",
  "/rkp?tab=profile",
  "/rkp?tab=verify",
  "/tricky-store",
  "/tricky-store/keybox",
  "/tricky-store/policy",
  "/tricky-store/props",
]
const variants = [
  { name: "compact", width: 320, height: 568, locale: "en", scale: 1 },
  { name: "long translations", width: 320, height: 568, locale: "de", scale: 1 },
  { name: "RTL", width: 320, height: 568, locale: "ar", scale: 1 },
  { name: "landscape", width: 568, height: 320, locale: "zh-CN", scale: 1 },
  { name: "large text", width: 412, height: 839, locale: "en", scale: 2 },
]

for (const variant of variants) {
  test.describe(variant.name, () => {
    test.use({ viewport: { width: variant.width, height: variant.height } })
    for (const route of routes) {
      test(`${route} fits the viewport`, async ({ page, open }) => {
        await open(route, { locale: variant.locale })
        await expect(page.getByRole("heading").first()).toBeVisible()
        await page.evaluate((scale) => {
          document.documentElement.style.fontSize = `${16 * scale}px`
        }, variant.scale)
        await expect(page.locator('main [aria-busy="true"]')).toHaveCount(0)
        const layout = await page.evaluate(() => {
          const width = document.documentElement.clientWidth
          const overflowing = [...document.querySelectorAll<HTMLElement>("main *")]
            .filter((element) => {
              const rect = element.getBoundingClientRect()
              return rect.width > 2 && (rect.right > width + 1 || rect.left < -1)
            })
            .slice(0, 6)
            .map(
              (element) =>
                `${element.tagName}[${element.getAttribute("data-slot") ?? ""}]: ${element.textContent?.trim().slice(0, 70)}`,
            )
          return { width, scroll: document.documentElement.scrollWidth, overflowing }
        })
        expect(layout.scroll, JSON.stringify(layout)).toBeLessThanOrEqual(layout.width + 1)
      })
    }

    test("confirmation remains within the screen and actions are reachable", async ({
      page,
      open,
    }) => {
      await open("/", { locale: "en" })
      await expect(page.getByRole("heading").first()).toBeVisible()
      await page.evaluate((scale) => {
        document.documentElement.style.fontSize = `${16 * scale}px`
      }, variant.scale)
      await page.getByRole("button", { name: /Device RKP Issuer/ }).click()
      const panel = page.getByRole("alertdialog")
      await expectPanelFits(page, panel)
      await panel.getByRole("button", { name: "Continue" }).scrollIntoViewIfNeeded()
      await expect(panel.getByRole("button", { name: "Continue" })).toBeInViewport()
    })

    test("provider editor scrolls without clipping fields or actions", async ({ page, open }) => {
      await open("/tricky-store/keybox", { locale: "en" })
      await expect(page.getByRole("button", { name: "Add provider" })).toBeVisible()
      await page.evaluate((scale) => {
        document.documentElement.style.fontSize = `${16 * scale}px`
      }, variant.scale)
      await page.getByRole("button", { name: "Add provider" }).click()
      const panel = page.getByRole("dialog")
      await expectPanelFits(page, panel)
      await panel.getByRole("button", { name: "Save", exact: true }).scrollIntoViewIfNeeded()
      await expect(panel.getByRole("button", { name: "Save", exact: true })).toBeInViewport()
    })

    test("target menu and mode sheet fit the screen", async ({ page, open }) => {
      await open("/tricky-store", { locale: "en" })
      await expect(page.getByText("4 selected")).toBeVisible()
      await page.evaluate((scale) => {
        document.documentElement.style.fontSize = `${16 * scale}px`
      }, variant.scale)
      await page.getByRole("button", { name: "More" }).click()
      await expectPanelFits(page, page.getByRole("menu"))
      await page.keyboard.press("Escape")
      await page.getByRole("button", { name: "Mode and policy of Key Attestation" }).click()
      const panel = page.getByRole("dialog")
      await expectPanelFits(page, panel)
      await panel.getByRole("button", { name: "Save", exact: true }).scrollIntoViewIfNeeded()
      await expect(panel.getByRole("button", { name: "Save", exact: true })).toBeInViewport()
    })
  })
}

for (const theme of ["light", "dark"] as const) {
  test(`supporting text has readable contrast on ${theme} surfaces`, async ({ page, open }) => {
    await open("/settings", { theme })
    await expect(page.getByRole("heading").first()).toBeVisible()
    const ratios = await page.evaluate(() => {
      const style = getComputedStyle(document.documentElement)
      const canvas = document.createElement("canvas")
      canvas.width = canvas.height = 1
      const context = canvas.getContext("2d")!
      function luminance(token: string) {
        context.fillStyle = style.getPropertyValue(token).trim()
        context.fillRect(0, 0, 1, 1)
        const [r, g, b] = [...context.getImageData(0, 0, 1, 1).data].slice(0, 3).map((value) => {
          const channel = value / 255
          return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4
        })
        return r! * 0.2126 + g! * 0.7152 + b! * 0.0722
      }
      const text = luminance("--muted-foreground")
      return Object.fromEntries(
        ["--background", "--card", "--secondary", "--popover"].map((token) => {
          const surface = luminance(token)
          return [token, (Math.max(text, surface) + 0.05) / (Math.min(text, surface) + 0.05)]
        }),
      )
    })
    for (const [surface, ratio] of Object.entries(ratios))
      expect(ratio, surface).toBeGreaterThanOrEqual(4.5)
  })
}
