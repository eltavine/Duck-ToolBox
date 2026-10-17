<script setup lang="ts">
import { useMutation, useQuery } from "@tanstack/vue-query"
import { Cpu, FileKey2, RefreshCw, ShieldCheck, TriangleAlert } from "@lucide/vue"
import { computed, ref } from "vue"
import { useI18n } from "vue-i18n"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import ConfirmDialog from "@/core/components/ConfirmDialog.vue"
import KeyValueList from "@/core/components/KeyValueList.vue"
import QueryState from "@/core/components/QueryState.vue"
import SectionCard from "@/core/components/SectionCard.vue"
import type { KeyboxTarget } from "@/core/features"
import { contributions } from "@/core/features"
import { notifySuccess } from "@/core/notify"

import type { Provision } from "../api"
import { rkpDeviceApi } from "../api"
import { useRkpDeviceI18n } from "../i18n"

interface Row {
  label: string
  value: string | number | null | undefined
  mono?: boolean
  copy?: boolean
}

const { t, dynamic } = useRkpDeviceI18n()
const global = useI18n()
const status = useQuery({
  queryKey: ["rkp-device.status"],
  queryFn: rkpDeviceApi.status,
  staleTime: 0,
})
const result = ref<Provision | null>(null)
const confirmOpen = ref(false)

// Other tools register where a generated keybox can be installed, like the Tricky Store
// manager does for the offline workbench.
const keyboxTargets = contributions("keyboxTargets")

const run = useMutation({
  mutationFn: rkpDeviceApi.provision,
  onSuccess: (data) => {
    result.value = data
    notifySuccess(t("run.done"))
  },
})

/** The target whose install is running, so only its button shows a spinner. */
const installingTarget = ref<string | null>(null)

const install = useMutation({
  mutationFn: (target: KeyboxTarget) => target.install(result.value!.artifacts.keybox_path),
  onMutate: (target) => {
    installingTarget.value = target.id
  },
  onSettled: () => {
    installingTarget.value = null
  },
  onSuccess: (installed) =>
    notifySuccess(
      t("run.installed", { target: installed.targetPath }),
      installed.backupPath ? t("run.backup", { path: installed.backupPath }) : undefined,
    ),
})

// Every limitation is a translatable sentence; the identifiers come from the backend,
// so they resolve through `dynamic` instead of the typed key union.
const limitationText = computed(() =>
  (result.value?.limitations ?? status.data.value?.limitations ?? []).map((id) =>
    dynamic(`limitations.${id}`, id),
  ),
)

const statusRows = computed<Row[]>(() => {
  const data = status.data.value
  if (!data) return []
  const yes = t("status.yes")
  const no = t("status.no")
  return [
    { label: t("status.platform"), value: data.android_arm64 ? yes : no },
    { label: t("status.root"), value: data.root ? yes : no },
    { label: t("status.vendor"), value: data.soc_manufacturer || t("status.unknown") },
    { label: t("status.socModel"), value: data.soc_detection.model || t("status.unknown") },
    {
      label: t("status.vendorOk"),
      value: {
        qualcomm: t("status.detected"),
        other: t("status.otherVendor"),
        unknown: t("status.unknown"),
        conflicting: t("status.conflicting"),
      }[data.soc_detection.state],
    },
    { label: t("status.library"), value: data.library_path, mono: true },
    { label: t("status.device"), value: data.device_path, mono: true },
    { label: t("status.fingerprint"), value: data.fingerprint },
    { label: t("status.compatibility"), value: t("status.unverified") },
  ]
})

function blockerText(blocker: string): string {
  return dynamic(`blockers.${blocker}`, blocker)
}

const resultRows = computed<Row[]>(() => {
  const data = result.value
  if (!data) return []
  return [
    { label: t("run.mode"), value: data.mode, mono: true },
    { label: t("run.fingerprint"), value: data.fingerprint },
    { label: t("run.challenge"), value: data.challenge_hex, mono: true },
    { label: t("run.csr"), value: data.artifacts.csr_path, mono: true, copy: true },
    {
      label: t("run.localVerify"),
      value: data.local_verify.signature_valid ? t("run.valid") : t("run.invalid"),
    },
    { label: t("run.certificates"), value: data.chain.certificates },
    { label: t("run.deviceId"), value: data.device_id, mono: true },
    { label: t("run.keybox"), value: data.artifacts.keybox_path, mono: true, copy: true },
    { label: t("run.privateKey"), value: data.artifacts.private_key_path, mono: true, copy: true },
    { label: t("run.certChain"), value: data.artifacts.cert_chain_path, mono: true, copy: true },
  ]
})
</script>

<template>
  <div class="flex flex-col gap-6 p-4">
    <QueryState
      :loading="status.isPending.value"
      :error="status.error.value"
      @retry="status.refetch()"
    >
      <template v-if="status.data.value">
        <!-- The two limitations are the first thing a user reads, not a footnote. -->
        <Alert>
          <TriangleAlert />
          <AlertTitle>{{ t("warning.title") }}</AlertTitle>
          <AlertDescription>
            <ul class="flex list-disc flex-col gap-1 ps-4">
              <li v-for="limitation in limitationText" :key="limitation">{{ limitation }}</li>
            </ul>
          </AlertDescription>
        </Alert>

        <SectionCard :title="t('status.title')" :description="t('status.description')">
          <template #action>
            <Button
              variant="ghost"
              size="icon"
              :aria-label="t('status.reload')"
              :disabled="status.isFetching.value || run.isPending.value"
              @click="status.refetch()"
            >
              <RefreshCw />
            </Button>
          </template>

          <div class="flex flex-col gap-4">
            <div class="flex flex-wrap items-center gap-2">
              <Badge :variant="status.data.value.prerequisites_met ? 'secondary' : 'destructive'">
                {{
                  status.data.value.prerequisites_met
                    ? t("status.prerequisitesMet")
                    : t("status.prerequisitesMissing")
                }}
              </Badge>
            </div>

            <Alert v-if="status.data.value.blockers.length" variant="destructive">
              <TriangleAlert />
              <AlertTitle>{{ t("status.blockers") }}</AlertTitle>
              <AlertDescription>
                <ul class="flex list-disc flex-col gap-1 ps-4">
                  <li v-for="blocker in status.data.value.blockers" :key="blocker">
                    {{ blockerText(blocker) }}
                  </li>
                </ul>
              </AlertDescription>
            </Alert>

            <KeyValueList :rows="statusRows" />
            <details v-if="status.data.value.soc_detection.evidence.length" class="min-w-0">
              <summary class="cursor-pointer text-sm font-medium">
                {{ t("status.evidence") }}
              </summary>
              <dl class="mt-3 flex min-w-0 flex-col gap-3">
                <div
                  v-for="item in status.data.value.soc_detection.evidence"
                  :key="item.source"
                  class="min-w-0"
                >
                  <dt class="text-muted-foreground font-mono text-xs break-all">
                    <bdi dir="ltr">{{ item.source }}</bdi>
                  </dt>
                  <dd class="mt-1 text-sm break-all">
                    <bdi dir="ltr">{{ item.value }}</bdi>
                  </dd>
                </div>
              </dl>
            </details>
          </div>
        </SectionCard>

        <Button
          size="lg"
          :disabled="
            run.isPending.value || status.isFetching.value || !status.data.value.prerequisites_met
          "
          @click="confirmOpen = true"
        >
          <Spinner v-if="run.isPending.value" class="size-6" />
          <Cpu v-else />
          {{ t("run.provision") }}
        </Button>

        <p v-if="!status.data.value.prerequisites_met" class="text-muted-foreground text-sm">
          {{ t("run.blocked") }}
        </p>
      </template>
    </QueryState>

    <template v-if="result">
      <SectionCard :title="t('run.title')">
        <KeyValueList :rows="resultRows" />
      </SectionCard>

      <div v-if="keyboxTargets.length" class="flex flex-col gap-2">
        <Button
          v-for="target in keyboxTargets"
          :key="target.id"
          variant="outline"
          size="lg"
          :disabled="install.isPending.value"
          @click="install.mutate(target)"
        >
          <Spinner v-if="installingTarget === target.id" class="size-6" />
          <FileKey2 v-else />
          {{ t("run.install", { target: global.t(target.labelKey) }) }}
        </Button>
      </div>

      <Alert>
        <ShieldCheck />
        <AlertTitle>{{ t("run.onlyRkp") }}</AlertTitle>
        <AlertDescription>{{ t("run.onlyRkpHint") }}</AlertDescription>
      </Alert>
    </template>

    <ConfirmDialog
      v-model:open="confirmOpen"
      :title="t('confirm.title')"
      :description="t('confirm.body')"
      :confirm-label="t('run.provision')"
      @confirm="run.mutate()"
    />
  </div>
</template>
