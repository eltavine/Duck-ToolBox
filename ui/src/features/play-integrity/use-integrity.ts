import { useQuery, useQueryClient } from "@tanstack/vue-query"
import { useStorage } from "@vueuse/core"
import { onScopeDispose, ref, watch } from "vue"
import { contributions } from "@/core/features"
import type { SecurityPatchPreview, SecurityPatchTarget } from "@/core/features"
import { notifyError, notifySuccess } from "@/core/notify"
import { integrityApi } from "./api"
import type { Options, Preview, Task } from "./api"
import { useIntegrityI18n } from "./i18n"

export function useIntegrity() {
  const { t } = useIntegrityI18n()
  const client = useQueryClient()
  const status = useQuery({
    queryKey: ["play-integrity.status"],
    queryFn: integrityApi.status,
    refetchOnWindowFocus: false,
  })
  const config = useQuery({
    queryKey: ["play-integrity.config"],
    queryFn: integrityApi.config,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  })
  const transaction = useQuery({
    queryKey: ["integrity-transaction.status"],
    queryFn: integrityApi.transactionStatus,
    refetchOnWindowFocus: true,
  })
  const content = ref("")
  const preview = ref<Preview | null>(null)
  const patches = ref<
    { target: SecurityPatchTarget; preview: SecurityPatchPreview; selected: boolean }[]
  >([])
  const task = ref<Task | null>(null)
  const taskId = useStorage("duck-toolbox/play-integrity-task", "")
  const backup = useStorage("duck-toolbox/play-integrity-backup", "")
  const options = ref<Options>({
    enabled: true,
    mode: "full",
    skipdelprop: false,
    skippersistprop: false,
    skipmatchdevice: false,
  })
  const busy = ref(false)
  const log = ref("")
  let timer: ReturnType<typeof setTimeout> | undefined
  let disposed = false
  let previewGeneration = 0
  let taskGeneration = 0
  function clearPreview() {
    previewGeneration++
    preview.value = null
    patches.value = []
  }
  watch(
    config.data,
    (value, previous) => {
      if (value && (!previous || content.value === previous.content)) content.value = value.content
    },
    { immediate: true },
  )
  watch(
    status.data,
    (value) => {
      if (value) options.value = { ...value.options }
    },
    { immediate: true },
  )
  watch(content, clearPreview)
  async function action(work: () => Promise<unknown>) {
    if (busy.value) return
    busy.value = true
    try {
      await work()
    } catch (error) {
      notifyError(error)
    } finally {
      busy.value = false
    }
  }
  async function refresh() {
    await Promise.all([
      client.invalidateQueries({ queryKey: ["integrity-transaction.status"] }),
      client.invalidateQueries({ queryKey: ["play-integrity.status"] }),
      client.invalidateQueries({ queryKey: ["play-integrity.config"] }),
      client.invalidateQueries({ queryKey: ["play-integrity.inspection"] }),
    ])
  }
  async function prepare(value: Preview) {
    clearPreview()
    const generation = previewGeneration
    const prepared: typeof patches.value = []
    if (value.suggested_patch) {
      for (const target of contributions("securityPatchTargets")) {
        const patch = await target.preview(value.suggested_patch)
        if (disposed || generation !== previewGeneration) return
        if (patch) prepared.push({ target, preview: patch, selected: false })
      }
    }
    if (disposed || generation !== previewGeneration) return
    patches.value = prepared
    preview.value = value
  }
  async function apply() {
    const candidate = preview.value
    if (!candidate) return
    const selected = patches.value.filter((item) => item.selected)
    if (selected.length > 1) throw new Error("Only one active keystore participant is supported")
    const patch = selected[0]
    try {
      const applied = await integrityApi.applyTransaction(
        candidate,
        patch
          ? {
              target: patch.target.id,
              patch: patch.preview.patch,
              revision: patch.preview.revision,
              partitions: patch.preview.partitions,
            }
          : undefined,
      )
      backup.value = applied.backup
      content.value = applied.config.content
      if (applied.restart_required)
        notifySuccess(t(patch ? "activation.reboot" : "activation.process"))
    } finally {
      await refresh()
    }
    preview.value = null
    await refresh()
    notifySuccess(t("saved"))
  }
  async function poll() {
    if (disposed || !taskId.value) return
    try {
      task.value = await integrityApi.taskStatus(taskId.value)
      if (task.value.state === "completed" && task.value.result) {
        taskId.value = ""
        if (taskGeneration === previewGeneration) await prepare(task.value.result)
      } else if (task.value.state !== "running") {
        taskId.value = ""
      } else timer = setTimeout(() => void poll(), 1000)
    } catch (error) {
      notifyError(error)
      taskId.value = ""
    }
  }
  if (taskId.value) void poll()
  onScopeDispose(() => {
    disposed = true
    clearPreview()
    if (timer) clearTimeout(timer)
  })
  return {
    t,
    status,
    transaction,
    config,
    content,
    preview,
    patches,
    task,
    taskId,
    backup,
    options,
    busy,
    log,
    action,
    refresh,
    apply,
    prepare,
    clearPreview,
    patchPreview: (id: string, partitions: Record<string, string>) =>
      action(async () => {
        const item = patches.value.find((p) => p.target.id === id)
        if (!item) return
        const next = await item.target.preview(item.preview.patch, partitions)
        if (!next) throw new Error("Keystore backend is no longer active")
        item.preview = next
      }),
    previewContent: (migrate = false) =>
      action(async () => prepare(await integrityApi.preview(content.value, "auto", migrate))),
    importSource: (source: string) =>
      action(async () => prepare(await integrityApi.importPreview(source))),
    saveOptions: (list: string) =>
      action(async () => {
        await integrityApi.options({
          mode: options.value.mode,
          skipdelprop: options.value.skipdelprop,
          skippersistprop: options.value.skippersistprop,
          skipmatchdevice: options.value.skipmatchdevice,
          app_replace_list: list,
        })
        await refresh()
        notifySuccess(t("reboot"))
      }),
    generate: (strong: boolean, match: boolean) =>
      action(async () => {
        clearPreview()
        taskGeneration = previewGeneration
        task.value = await integrityApi.taskStart(strong, match)
        taskId.value = task.value.id
        void poll()
      }),
    cancel: () =>
      action(async () => {
        if (taskId.value) task.value = await integrityApi.taskCancel(taskId.value)
      }),
    restore: () =>
      action(async () => {
        if (backup.value) {
          const restored = await integrityApi.restore(backup.value)
          content.value = restored.config.content
          backup.value = ""
          await refresh()
        }
      }),
    restart: () =>
      action(async () => {
        const result = await integrityApi.restart()
        log.value = JSON.stringify(result.processes, null, 2)
        await refresh()
        notifySuccess(t("restartSent"))
      }),
    diagnose: () =>
      action(async () => {
        log.value = (await integrityApi.diagnose()).log
      }),
  }
}
