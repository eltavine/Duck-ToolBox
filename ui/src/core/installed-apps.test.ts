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
function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  let query!: ReturnType<typeof useInstalledApps>
  const wrapper = mount(
    defineComponent({
      setup() {
        query = useInstalledApps()
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
