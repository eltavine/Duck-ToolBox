import { QueryClient, VueQueryPlugin } from "@tanstack/vue-query"
import { flushPromises, mount } from "@vue/test-utils"
import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { defineComponent, h } from "vue"
import type { SecurityPatchPreview, SecurityPatchTarget } from "@/core/features"
import { notifyError } from "@/core/notify"
import { integrityApi } from "./api"
import { libraryApi } from "./library-api"
import type { Preview } from "./api"
import { useIntegrity } from "./use-integrity"

const { targets } = vi.hoisted(() => ({ targets: [] as SecurityPatchTarget[] }))
vi.mock("@/core/features", () => ({ contributions: () => targets }))
vi.mock("./i18n", () => ({ useIntegrityI18n: () => ({ t: (key: string) => key }) }))
vi.mock("@/core/notify", () => ({ notifyError: vi.fn(), notifySuccess: vi.fn() }))
vi.mock("./api", () => ({
  integrityApi: {
    status: vi.fn(async () => ({ options: { enabled: true } })),
    config: vi.fn(async () => ({ content: "MODEL=Saved", entries: {}, revision: "saved" })),
    transactionStatus: vi.fn(async () => ({ pending: false })),
    taskStart: vi.fn(),
    taskStatus: vi.fn(),
    taskCancel: vi.fn(),
    preview: vi.fn(),
    importPreview: vi.fn(),
  },
}))
vi.mock("./library-api", () => ({ libraryApi: { profilePreview: vi.fn() } }))

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (cause: unknown) => void
  const promise = new Promise<T>((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}
const candidate = (model: string, patch: string | null = null): Preview => ({
  content: `MODEL=${model}`,
  json: JSON.stringify({ MODEL: model }),
  entries: { MODEL: model },
  revision: "saved",
  changes: [],
  warnings: [],
  suggested_patch: patch,
})
const cleanups: (() => void)[] = []
beforeEach(() => {
  targets.length = 0
  localStorage.clear()
})
afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  vi.clearAllMocks()
})
async function setup() {
  let state!: ReturnType<typeof useIntegrity>
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(
    defineComponent({
      setup() {
        state = useIntegrity()
        return () => h("div")
      },
    }),
    {
      global: { plugins: [[VueQueryPlugin, { queryClient: client }]] },
    },
  )
  cleanups.push(() => {
    wrapper.unmount()
    client.clear()
  })
  await flushPromises()
  return state
}

it("does not publish a dismissed preview after a slow keystore response", async () => {
  const pending = deferred<SecurityPatchPreview | null>()
  targets.push({ id: "keystore", labelKey: "test", preview: () => pending.promise })
  const state = await setup()
  const preparing = state.prepare(candidate("Archive", "2026-10-05"))
  state.clearPreview()
  pending.resolve(null)
  await preparing
  expect(state.preview.value).toBeNull()
  expect(state.patches.value).toEqual([])
})

it("keeps the later selection when an earlier preview finishes last", async () => {
  const pending = deferred<SecurityPatchPreview | null>()
  targets.push({ id: "keystore", labelKey: "test", preview: () => pending.promise })
  const state = await setup()
  const preparing = state.prepare(candidate("Old", "2026-10-05"))
  await state.prepare(candidate("New"))
  pending.resolve(null)
  await preparing
  expect(state.preview.value?.entries.MODEL).toBe("New")
})

it("leaves no partially prepared candidate after a keystore error", async () => {
  targets.push({
    id: "keystore",
    labelKey: "test",
    preview: async () => {
      throw new Error("Unavailable")
    },
  })
  const state = await setup()
  await expect(state.prepare(candidate("Archive", "2026-10-05"))).rejects.toThrow("Unavailable")
  expect(state.preview.value).toBeNull()
  expect(state.patches.value).toEqual([])
})

it("retains a finished generator result without overwriting a later archive selection", async () => {
  const pending = deferred<Awaited<ReturnType<typeof integrityApi.taskStatus>>>()
  vi.mocked(integrityApi.taskStart).mockResolvedValue({
    id: "job",
    state: "running",
    result: null,
    error: null,
    output: "",
  })
  vi.mocked(integrityApi.taskStatus).mockReturnValue(pending.promise)
  const state = await setup()
  await state.generate(true, false)
  await state.prepare(candidate("Archive"))
  pending.resolve({
    id: "job",
    state: "completed",
    result: candidate("Generated"),
    error: null,
    output: "",
  })
  await flushPromises()
  expect(state.preview.value?.entries.MODEL).toBe("Archive")
  expect(state.task.value?.result?.entries.MODEL).toBe("Generated")
})

for (const stale of ["unmounted", "replaced", "failed"] as const) {
  it(`ignores a ${stale} polling request without clearing the tracked job`, async () => {
    const pending = deferred<Awaited<ReturnType<typeof integrityApi.taskStatus>>>()
    vi.mocked(integrityApi.taskStart).mockResolvedValue({
      id: "old",
      state: "running",
      result: null,
      error: null,
      output: "",
    })
    vi.mocked(integrityApi.taskStatus).mockReturnValue(pending.promise)
    const state = await setup()
    await state.generate(true, false)
    if (stale === "unmounted") cleanups.pop()!()
    else state.taskId.value = "new"
    if (stale === "failed") pending.reject(new Error("Late failure"))
    else
      pending.resolve({
        id: "old",
        state: "completed",
        result: candidate("Old"),
        error: null,
        output: "",
      })
    await flushPromises()
    expect(state.taskId.value).toBe(stale === "unmounted" ? "old" : "new")
    expect(state.task.value?.state).toBe("running")
    expect(state.preview.value).toBeNull()
    expect(notifyError).not.toHaveBeenCalled()
  })
}

it("ignores a cancelled old job's response after another job becomes current", async () => {
  const status = deferred<Awaited<ReturnType<typeof integrityApi.taskStatus>>>()
  const cancelled = deferred<Awaited<ReturnType<typeof integrityApi.taskCancel>>>()
  vi.mocked(integrityApi.taskStart).mockResolvedValue({
    id: "old",
    state: "running",
    result: null,
    error: null,
    output: "",
  })
  vi.mocked(integrityApi.taskStatus).mockReturnValue(status.promise)
  vi.mocked(integrityApi.taskCancel).mockReturnValue(cancelled.promise)
  const state = await setup()
  await state.generate(true, false)
  const request = state.cancel()
  state.taskId.value = "new"
  cancelled.resolve({ id: "old", state: "cancelled", result: null, error: null, output: "" })
  await request
  expect(state.taskId.value).toBe("new")
  expect(state.task.value?.state).toBe("running")
})

it("keeps a completed job tracked when its patch preview outlives the page", async () => {
  const patch = deferred<SecurityPatchPreview | null>()
  targets.push({ id: "keystore", labelKey: "test", preview: () => patch.promise })
  vi.mocked(integrityApi.taskStart).mockResolvedValue({
    id: "job",
    state: "running",
    result: null,
    error: null,
    output: "",
  })
  vi.mocked(integrityApi.taskStatus).mockResolvedValue({
    id: "job",
    state: "completed",
    result: candidate("Generated", "2026-10-05"),
    error: null,
    output: "",
  })
  const state = await setup()
  await state.generate(true, false)
  await flushPromises()
  cleanups.pop()!()
  patch.resolve(null)
  await flushPromises()
  expect(state.taskId.value).toBe("job")
  expect(state.preview.value).toBeNull()
})

for (const source of ["editor", "import", "profile"] as const) {
  for (const outcome of ["edited", "replaced", "unmounted", "failed"] as const) {
    it(`discards a ${outcome} ${source} preview response`, async () => {
      const pending = deferred<Preview>()
      const load =
        source === "editor"
          ? integrityApi.preview
          : source === "import"
            ? integrityApi.importPreview
            : libraryApi.profilePreview
      vi.mocked(load).mockReturnValue(pending.promise)
      const state = await setup()
      const request =
        source === "editor"
          ? state.previewContent()
          : source === "import"
            ? state.importSource("/source")
            : state.previewProfile("snapshot")
      if (outcome === "edited") state.content.value = "MODEL=New draft"
      else if (outcome === "unmounted") cleanups.pop()!()
      else await state.prepare(candidate("Later"))
      if (outcome === "failed") pending.reject(new Error("Stale failure"))
      else pending.resolve(candidate("Old"))
      await request
      expect(state.preview.value?.entries.MODEL ?? null).toBe(
        outcome === "replaced" || outcome === "failed" ? "Later" : null,
      )
      expect(notifyError).not.toHaveBeenCalled()
      expect(state.busy.value).toBe(false)
    })
  }
}

it("reports current request and patch failures while suppressing a dismissed patch failure", async () => {
  const state = await setup()
  vi.mocked(integrityApi.preview).mockRejectedValueOnce(new Error("Current request"))
  await state.previewContent()
  expect(notifyError).toHaveBeenLastCalledWith(
    expect.objectContaining({ message: "Current request" }),
  )
  targets.push({
    id: "keystore",
    labelKey: "test",
    preview: async () => {
      throw new Error("Current patch")
    },
  })
  vi.mocked(integrityApi.preview).mockResolvedValue(candidate("Current", "2026-10-05"))
  await state.previewContent()
  expect(notifyError).toHaveBeenLastCalledWith(
    expect.objectContaining({ message: "Current patch" }),
  )
  const pending = deferred<SecurityPatchPreview | null>()
  targets.splice(0, 1, { id: "keystore", labelKey: "test", preview: () => pending.promise })
  const preparing = state.prepare(candidate("Old", "2026-10-05"))
  state.clearPreview()
  pending.reject(new Error("Dismissed patch"))
  await preparing
  expect(notifyError).toHaveBeenCalledTimes(2)
})

for (const failed of [false, true]) {
  it(`ignores a replaced partition preview ${failed ? "error" : "result"}`, async () => {
    const pending = deferred<SecurityPatchPreview | null>()
    const initial: SecurityPatchPreview = {
      backend: "test",
      patch: "2026-10-05",
      revision: "old",
      changes: [],
    }
    let first = true
    targets.push({
      id: "keystore",
      labelKey: "test",
      preview: async () => {
        if (first) {
          first = false
          return initial
        }
        return pending.promise
      },
    })
    const state = await setup()
    await state.prepare(candidate("Old", initial.patch))
    const request = state.patchPreview("keystore", { vendor: "2026-09-05" })
    await state.prepare(candidate("Later"))
    if (failed) pending.reject(new Error("Stale partition error"))
    else pending.resolve({ ...initial, revision: "changed" })
    await request
    expect(state.preview.value?.entries.MODEL).toBe("Later")
    expect(state.patches.value).toEqual([])
    expect(notifyError).not.toHaveBeenCalled()
  })
}
