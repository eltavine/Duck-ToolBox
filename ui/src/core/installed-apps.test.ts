import { QueryClient, VueQueryPlugin } from "@tanstack/vue-query"
import { mount, flushPromises } from "@vue/test-utils"
import { afterEach, expect, it, vi } from "vitest"
import { defineComponent, h } from "vue"
import { bridge } from "./bridge"
import { useInstalledApps } from "./installed-apps"

vi.mock("./bridge", () => ({ bridge: vi.fn() }))
const cleanups: (() => void)[] = []
afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  vi.resetAllMocks()
})
function setup(scope?: Parameters<typeof useInstalledApps>[0]) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  let query!: ReturnType<typeof useInstalledApps>
  const wrapper = mount(
    defineComponent({
      setup() {
        query = useInstalledApps(scope)
        return () => h("div")
      },
    }),
    { global: { plugins: [[VueQueryPlugin, { queryClient: client }]] } },
  )
  cleanups.push(() => {
    wrapper.unmount()
    client.clear()
  })
  return { query, wrapper }
}

it("deduplicates package names and retains apps without metadata", async () => {
  const listPackages = vi.fn().mockResolvedValue(["com.z", "com.a", "com.z", "com.b"])
  const packagesInfo = vi.fn().mockResolvedValue([
    { packageName: "com.z", label: "Alpha" },
    { packageName: "com.a", label: "" },
  ])
  vi.mocked(bridge).mockReturnValue({ listPackages, packagesInfo } as unknown as ReturnType<
    typeof bridge
  >)
  const { query } = setup()
  await flushPromises()
  expect(listPackages).toHaveBeenCalledWith("all")
  expect(packagesInfo.mock.calls[0]![0]).toEqual(["com.z", "com.a", "com.b"])
  expect(query.data.value).toEqual([
    { packageName: "com.z", label: "Alpha" },
    { packageName: "com.a", label: "com.a" },
    { packageName: "com.b", label: "com.b" },
  ])
})

it("does not start metadata batches after the last picker is unmounted", async () => {
  let finish!: (names: string[]) => void
  const listPackages = vi.fn(
    () =>
      new Promise<string[]>((resolve) => {
        finish = resolve
      }),
  )
  const packagesInfo = vi.fn()
  vi.mocked(bridge).mockReturnValue({ listPackages, packagesInfo } as unknown as ReturnType<
    typeof bridge
  >)
  const { wrapper } = setup()
  wrapper.unmount()
  finish(["com.example.app"])
  await flushPromises()
  expect(packagesInfo).not.toHaveBeenCalled()
})

it("exposes each known user to scoped callers and one row to package callers", async () => {
  const listPackages = vi.fn().mockResolvedValue(["com.example.app"])
  const packagesInfo = vi.fn().mockResolvedValue([
    { packageName: "com.example.app", label: "Example" },
    { packageName: "com.example.app", label: "Example", userId: 999 },
    { packageName: "com.example.app", label: "Example", userId: 0 },
    { packageName: "com.example.app", label: "Example", userId: 999 },
  ])
  vi.mocked(bridge).mockReturnValue({ listPackages, packagesInfo } as unknown as ReturnType<
    typeof bridge
  >)
  const { query: users } = setup("user")
  const { query: packages } = setup()
  await flushPromises()
  expect(users.data.value).toEqual([
    { packageName: "com.example.app", label: "Example", userId: 0 },
    { packageName: "com.example.app", label: "Example", userId: 999 },
  ])
  expect(packages.data.value).toEqual([{ packageName: "com.example.app", label: "Example" }])
})
