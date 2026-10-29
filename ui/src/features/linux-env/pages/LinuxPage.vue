<script setup lang="ts">
import { computed, ref } from "vue"
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query"
import { RefreshCw } from "@lucide/vue"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import SectionCard from "@/core/components/SectionCard.vue"
import KeyValueList from "@/core/components/KeyValueList.vue"
import QueryState from "@/core/components/QueryState.vue"
import ConfirmDialog from "@/core/components/ConfirmDialog.vue"
import { useCopy } from "@/core/links"
import { shellQuote } from "@/core/bridge"
import { moduleDir } from "@/core/duckd"
import { errorTitle, showErrorDetails, notifySuccess } from "@/core/notify"
import { formatBytes } from "@/core/format"
import { linuxApi, configSchema, viewSchema } from "../api"
import type { Config, EnvironmentView } from "../api"
import { useLinuxI18n } from "../i18n"
import ImportRootfs from "../components/ImportRootfs.vue"
import EnvironmentConfig from "../components/EnvironmentConfig.vue"
const { t, dynamic, locale } = useLinuxI18n()
const client = useQueryClient(),
  copy = useCopy(),
  root = ref<HTMLElement | null>(null)
const status = useQuery({
  queryKey: ["linux-env"],
  queryFn: linuxApi.status,
  refetchInterval: (query) => {
    const data = query.state.data
    return data &&
      ((data.job && !["done", "failed"].includes(data.job.phase)) ||
        data.environments.some((v) =>
          ["starting", "running", "stopping", "unknown"].includes(v.phase),
        ))
      ? data.job && !["done", "failed"].includes(data.job.phase)
        ? 1500
        : 3000
      : false
  },
})
const allowed = computed(
  () => !!status.data.value?.supported && !status.data.value.busy && !status.isError.value,
)
type Pending = {
  action: "start" | "stop" | "remove" | "save"
  view: EnvironmentView
  config?: Config
}
const dialog = ref(false),
  pending = ref<Pending | null>(null)
const logs = ref<{ id: string; text: string } | null>(null)
const refresh = () => client.invalidateQueries({ queryKey: ["linux-env"] })
const operation = useMutation({
  onError: () => undefined,
  mutationFn: async (request: Pending) => {
    if (request.action === "save" && request.config)
      await linuxApi.save(request.view, request.config)
    else if (request.action !== "save") await linuxApi.action(request.action, request.view)
  },
  onSuccess: (_data, request) => {
    if (request.action === "save") notifySuccess(t("done"))
  },
  onSettled: refresh,
})
const cancellation = useMutation({ mutationFn: linuxApi.cancel, onSettled: refresh })
const logger = useMutation({
  mutationFn: async (id: string) => {
    logs.value = { id, text: await linuxApi.logs(id) }
  },
})
let returnFocus: HTMLElement | null = null
let accepted = false
function request(
  action: "start" | "stop" | "remove" | "save",
  view: EnvironmentView,
  config?: Config,
) {
  returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
  accepted = false
  pending.value = {
    action,
    view: viewSchema.parse(view),
    config: config ? configSchema.parse(config) : undefined,
  }
  dialog.value = true
}
function confirmOperation() {
  accepted = true
  if (pending.value) operation.mutate(pending.value)
}
function focus(event: Event) {
  event.preventDefault()
  const target = !accepted && returnFocus?.isConnected ? returnFocus : root.value
  target?.focus({ preventScroll: true })
}
const busy = computed(() => operation.isPending.value || cancellation.isPending.value)
const importing = computed(
  () => status.data.value?.job && !["done", "failed"].includes(status.data.value.job.phase),
)
function terminal(id: string) {
  return `su -c ${shellQuote(`${shellQuote(`${moduleDir()}/bin/duckd`)} linux-env shell ${shellQuote(id)}`)}`
}
</script>
<template>
  <div
    ref="root"
    tabindex="-1"
    data-testid="linux-env-page"
    class="flex min-w-0 flex-col gap-6 p-4 outline-none"
  >
    <SectionCard :title="t('overview')" :description="t('trust')">
      <template #action
        ><Button
          variant="ghost"
          size="icon"
          class="min-h-12 min-w-12"
          :aria-label="t('refresh')"
          :disabled="status.isFetching.value || busy || dialog"
          @click="refresh"
          ><RefreshCw :class="{ 'motion-safe:animate-spin': status.isFetching.value }" /></Button
      ></template>
      <QueryState :loading="status.isPending.value" :error="status.error.value" @retry="refresh">
        <p v-if="!status.data.value?.supported" class="text-muted-foreground text-sm">
          {{ t("unsupported") }}
        </p>
        <p v-else-if="!status.data.value.runtime_ready" class="text-destructive text-sm">
          {{ t("missingRuntime") }}
        </p>
        <p v-else class="text-sm">{{ t("ready") }}</p>
      </QueryState>
    </SectionCard>
    <SectionCard
      v-if="operation.error.value"
      :title="errorTitle(operation.error.value)"
      role="alert"
      ><Button
        variant="outline"
        class="min-h-12"
        @click="showErrorDetails(operation.error.value)"
        >{{ t("details") }}</Button
      ></SectionCard
    >
    <SectionCard v-if="status.data.value?.job" :title="t('import')">
      <p role="status">
        {{
          status.data.value.job.code === "cancelled"
            ? t("phase.cancelled")
            : dynamic(`phase.${status.data.value.job.phase}`, status.data.value.job.phase)
        }}
      </p>
      <KeyValueList
        :rows="[
          { label: t('size'), value: formatBytes(status.data.value.job.bytes) },
          {
            label: t('entries'),
            value: new Intl.NumberFormat(locale).format(status.data.value.job.entries),
          },
        ]"
      />
      <p v-if="status.data.value.job.message" class="mt-3 text-sm break-words" dir="auto">
        {{ status.data.value.job.message }}
      </p>
      <Button
        v-if="importing"
        variant="outline"
        class="mt-3 min-h-12"
        :disabled="busy || !status.data.value.supported"
        @click="cancellation.mutate(status.data.value.job.token)"
        >{{ t("cancel") }}</Button
      >
    </SectionCard>
    <ImportRootfs
      :disabled="!allowed"
      :busy="busy || !!importing || !!status.data.value?.busy"
      @imported="
        async () => {
          await refresh()
          root?.focus({ preventScroll: true })
        }
      "
    />
    <p
      v-if="status.data.value && !status.data.value.environments.length"
      class="text-muted-foreground text-sm"
    >
      {{ t("empty") }}
    </p>
    <template
      v-for="view in status.data.value?.environments"
      :key="`${view.environment.id}:${view.environment.instance}`"
    >
      <SectionCard :title="view.environment.config.name">
        <template #action
          ><Badge variant="secondary">{{
            dynamic(`phase.${view.phase}`, view.phase)
          }}</Badge></template
        >
        <KeyValueList
          :rows="[
            { label: t('id'), value: view.environment.id, mono: true },
            { label: t('size'), value: formatBytes(view.environment.expanded_bytes) },
            { label: 'SHA-256', value: view.environment.archive_sha256, mono: true },
          ]"
        />
        <p v-if="view.message" class="mt-3 text-sm break-words" dir="auto">{{ view.message }}</p>
        <div class="mt-4 flex flex-wrap gap-2">
          <Button
            class="min-h-12"
            :disabled="
              !allowed ||
              busy ||
              !status.data.value?.runtime_ready ||
              !['stopped', 'failed'].includes(view.phase)
            "
            @click="request('start', view)"
            >{{ t("start") }}</Button
          >
          <Button
            variant="outline"
            class="min-h-12"
            :disabled="
              !allowed ||
              busy ||
              !['starting', 'running', 'stopping', 'unknown'].includes(view.phase)
            "
            @click="request('stop', view)"
            >{{ t("stop") }}</Button
          >
          <Button
            variant="outline"
            class="min-h-12"
            :disabled="view.phase !== 'running'"
            @click="copy(terminal(view.environment.id))"
            >{{ t("terminal") }}</Button
          >
          <Button
            variant="outline"
            class="min-h-12"
            :disabled="logger.isPending.value"
            @click="logger.mutate(view.environment.id)"
            >{{ t("logs") }}</Button
          >
          <Button
            variant="destructive"
            class="min-h-12"
            :disabled="!allowed || busy || !['stopped', 'failed'].includes(view.phase)"
            @click="request('remove', view)"
            >{{ t("remove") }}</Button
          >
        </div>
        <details class="mt-4">
          <summary class="min-h-12 cursor-pointer py-3">{{ t("configure") }}</summary>
          <EnvironmentConfig
            :id="view.environment.id"
            :revision="view.revision"
            :config="view.environment.config"
            :disabled="!allowed || busy"
            @save="(config) => request('save', view, config)"
          />
        </details>
        <pre
          v-if="logs?.id === view.environment.id"
          class="bg-muted mt-4 max-h-96 max-w-full overflow-auto rounded-lg p-4 text-xs"
          dir="ltr"
          tabindex="0"
          :aria-label="t('logs')"
          >{{ logs.text || t("emptyLogs") }}</pre>
      </SectionCard>
    </template>
    <ConfirmDialog
      v-model:open="dialog"
      :title="t(pending?.action ?? 'start')"
      :confirm-label="t(pending?.action ?? 'start')"
      :description="
        pending?.action === 'remove'
          ? t('deleteConfirm')
          : pending?.action === 'stop'
            ? t('stopConfirm')
            : t('trust')
      "
      :destructive="pending?.action === 'remove' || pending?.action === 'stop'"
      @confirm="confirmOperation"
      @close-auto-focus="focus"
      ><KeyValueList
        :rows="[
          {
            label: t('name'),
            value: pending?.config?.name ?? pending?.view.environment.config.name,
          },
          { label: t('id'), value: pending?.view.environment.id, mono: true },
        ]"
      />
      <template v-if="pending?.action === 'save' && pending.config">
        <p class="mt-4 text-sm">{{ t("configHelp") }}</p>
        <KeyValueList
          class="mt-3"
          :rows="[
            { label: t('boot'), value: pending.config.auto_start ? t('enabled') : t('disabled') },
            {
              label: t('dns'),
              value: pending.config.dns.length ? pending.config.dns.join(', ') : t('defaultDns'),
              mono: !!pending.config.dns.length,
            },
          ]"
        />
        <p v-if="!pending.config.services.length" class="mt-3 text-sm">{{ t("noServices") }}</p>
        <div v-for="service in pending.config.services" :key="service.id" class="mt-3 text-sm">
          <bdi class="font-mono" dir="ltr">{{ service.id }}</bdi
          ><span> · {{ service.enabled ? t("enabled") : t("disabled") }}</span>
          <pre
            class="bg-muted mt-1 max-h-48 overflow-auto rounded-lg p-3 text-xs"
            dir="ltr"
            tabindex="0"
            :aria-label="t('command')"
            >{{ service.command }}</pre>
        </div>
      </template></ConfirmDialog
    >
  </div>
</template>
