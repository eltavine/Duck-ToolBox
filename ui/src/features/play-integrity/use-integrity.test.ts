import { QueryClient, VueQueryPlugin } from "@tanstack/vue-query"
import { flushPromises, mount } from "@vue/test-utils"
import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { defineComponent, h } from "vue"
import type { SecurityPatchPreview, SecurityPatchTarget } from "@/core/features"
import { notifyError } from "@/core/notify"
import { integrityApi } from "./api"
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
  },
}))

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
