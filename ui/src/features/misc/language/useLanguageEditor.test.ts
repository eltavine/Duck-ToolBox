import { QueryClient, VueQueryPlugin } from "@tanstack/vue-query"
import { flushPromises, mount } from "@vue/test-utils"
import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { defineComponent, h } from "vue"
import { notifyError } from "@/core/notify"
import { languageApi, type LanguageStatus } from "./api"
import { useLanguageEditor } from "./useLanguageEditor"

vi.mock("vue-router", () => ({ onBeforeRouteLeave: vi.fn() }))
vi.mock("vue-i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock("../i18n", () => ({ useMiscI18n: () => ({ t: (key: string) => key }) }))
vi.mock("@/core/notify", () => ({ notifyError: vi.fn(), notifySuccess: vi.fn() }))
vi.mock("./api", () => ({
  languageApi: { status: vi.fn(), save: vi.fn(), enable: vi.fn(), disable: vi.fn() },
}))
const key = ["misc", "language"]
const cleanups: (() => void)[] = []
const value = (revision: string, enabled = false, locale = "en-US"): LanguageStatus => ({
  revision,
  enabled,
  config: { rules: [{ package_name: "com.example.app", user_id: 0, locale }] },
  config_error: null,
  parent_active: true,
  payload_installed: true,
  restart_required: true,
})
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}
beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(languageApi.status).mockResolvedValue(value("original"))
  vi.mocked(languageApi.disable).mockResolvedValue({ enabled: false, restart_required: true })
})
afterEach(() => cleanups.splice(0).forEach((cleanup) => cleanup()))
async function setup() {
  let editor!: ReturnType<typeof useLanguageEditor>
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(
    defineComponent({
      setup() {
        editor = useLanguageEditor()
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
  return { editor, client, wrapper }
}

it("locks editing until an explicit refresh completes", async () => {
  const { editor } = await setup()
  const response = deferred<LanguageStatus>()
  vi.mocked(languageApi.status).mockReturnValueOnce(response.promise)
  const refreshing = editor.refresh()
  await flushPromises()
  expect(editor.busy.value).toBe(true)
  response.resolve(value("refreshed"))
  await refreshing
  expect(editor.busy.value).toBe(false)
  expect(editor.status.data.value?.revision).toBe("refreshed")
})

it("uses the saved response as authoritative state without a second read", async () => {
  const { editor } = await setup()
  editor.draft.value[0]!.locale = "ja-JP"
  vi.mocked(languageApi.save).mockResolvedValue(value("saved", false, "ja-JP"))
  await editor.save()
  await flushPromises()
  expect(editor.status.data.value?.revision).toBe("saved")
  expect(editor.changed.value).toBe(false)
  expect(editor.dirty.value).toBe(false)
  expect(languageApi.status).toHaveBeenCalledTimes(1)
})

it("retains the revision from before an enable confirmation opened", async () => {
  const { editor, client } = await setup()
  const enabling = editor.enable()
  await flushPromises()
  client.setQueryData(key, value("stopped-elsewhere"))
  await flushPromises()
  vi.mocked(languageApi.enable).mockRejectedValue(new Error("stale revision"))
  editor.confirmation.accept()
  await enabling
  expect(languageApi.enable).toHaveBeenCalledWith("original")
  expect(notifyError).toHaveBeenCalled()
})

it("shows a successful stop even when its readback fails and retains local edits", async () => {
  vi.mocked(languageApi.status).mockResolvedValue(value("original", true))
  const { editor } = await setup()
  editor.draft.value[0]!.locale = "ja-JP"
  vi.mocked(languageApi.status).mockRejectedValueOnce(new Error("readback failed"))
  await editor.disable()
  await flushPromises()
  expect(editor.status.data.value?.enabled).toBe(false)
  expect(editor.status.isError.value).toBe(true)
  expect(editor.draft.value[0]?.locale).toBe("ja-JP")
  expect(notifyError).toHaveBeenCalled()
})

it("can save retained local edits after an acknowledged stop", async () => {
  const { editor } = await setup()
  editor.draft.value[0]!.locale = "ja-JP"
  vi.mocked(languageApi.status).mockResolvedValueOnce(value("stopped"))
  await editor.disable()
  await flushPromises()
  expect(editor.dirty.value).toBe(true)
  expect(editor.changed.value).toBe(false)
  vi.mocked(languageApi.save).mockResolvedValue(value("saved", false, "ja-JP"))
  await editor.save()
  expect(languageApi.save).toHaveBeenCalledWith("stopped", expect.any(Array))
})

it("cancels pending confirmation when the editor is removed", async () => {
  const { editor, wrapper } = await setup()
  const enabling = editor.enable()
  await flushPromises()
  wrapper.unmount()
  await enabling
  expect(languageApi.enable).not.toHaveBeenCalled()
})
