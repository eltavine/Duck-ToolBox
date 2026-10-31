import { QueryClient, VueQueryPlugin } from "@tanstack/vue-query"
import { flushPromises, mount } from "@vue/test-utils"
import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { defineComponent, h } from "vue"
import { fakerApi, type Preview } from "./api"
import { useFakerEditor } from "./useFakerEditor"

vi.mock("./i18n", () => ({ useFakerI18n: () => ({ t: (key: string) => key }) }))
vi.mock("./api", () => ({
  fakerApi: {
    status: vi.fn(),
    preview: vi.fn(),
    importPreview: vi.fn(),
    restorePreview: vi.fn(),
    disable: vi.fn(),
  },
}))
const cleanups: (() => void)[] = []
const content = '[[apps]]\npackage="com.example.app"\nmodel="Phone"\n'
const candidate: Preview = {
  content,
  revision: "saved",
  summary: {
    templates: [],
    targets: ["com.example.app"],
    warnings: [],
    global_effects: false,
  },
}
beforeEach(() => {
  vi.resetAllMocks()
  sessionStorage.clear()
  vi.mocked(fakerApi.status).mockResolvedValue({
    enabled: true,
    stop_revision: "initial",
    parent_active: true,
    startup_ready: true,
    startup_failed: false,
    startup_error: null,
    cleanup_pending: false,
    global_recovery_requires_reboot: false,
    payload_installed: true,
    payload_active: true,
    payload_pending: false,
    conflict: false,
    injection_verified: false,
    legacy_available: false,
    upstream_revision: "test",
    config: { ...candidate, error: null },
  })
})
afterEach(() => cleanups.splice(0).forEach((cleanup) => cleanup()))
async function setup() {
  let editor!: ReturnType<typeof useFakerEditor>
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(
    defineComponent({
      setup() {
        editor = useFakerEditor()
        return () => h("div")
      },
    }),
    { global: { plugins: [[VueQueryPlugin, { queryClient: client }]] } },
  )
  cleanups.push(() => {
    wrapper.unmount()
    client.clear()
  })
  await flushPromises()
  return { editor, wrapper }
}
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

it.each(["prepare", "importLegacy", "restore"] as const)(
  "discards %s results after a draft changes and changes back",
  async (operation) => {
    const { editor } = await setup()
    const pending = deferred<Preview>()
    vi.mocked(fakerApi.preview).mockReturnValue(pending.promise)
    vi.mocked(fakerApi.importPreview).mockReturnValue(pending.promise)
    vi.mocked(fakerApi.restorePreview).mockReturnValue(pending.promise)
    const requesting = editor[operation]()
    editor.draft.value = "changed"
    editor.draft.value = content
    pending.resolve(candidate)
    await requesting
    expect(editor.preview.value).toBeNull()
  },
)
it("does not publish a preview after its page is closed", async () => {
  const { editor, wrapper } = await setup()
  const pending = deferred<Preview>()
  vi.mocked(fakerApi.preview).mockReturnValue(pending.promise)
  const requesting = editor.prepare()
  wrapper.unmount()
  pending.resolve(candidate)
  await requesting
  expect(editor.preview.value).toBeNull()
})
it("retains an acknowledged stop when status readback fails", async () => {
  const { editor } = await setup()
  vi.mocked(fakerApi.disable).mockResolvedValue({ enabled: false, stop_revision: "stopped" })
  vi.mocked(fakerApi.status).mockRejectedValue(new Error("readback unavailable"))
  await editor.disable()
  expect(editor.data.value?.enabled).toBe(false)
  expect(editor.data.value?.stop_revision).toBe("stopped")
  expect(editor.notice.value).toBe("stopHelp")
  expect(editor.status.isError.value).toBe(true)
})
