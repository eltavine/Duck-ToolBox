<script setup lang="ts">
import { useQuery } from "@tanstack/vue-query"

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import KeyValueList from "@/core/components/KeyValueList.vue"
import QueryState from "@/core/components/QueryState.vue"

import { kernelModulesApi } from "../api"
import { useKernelModulesI18n } from "../i18n"

const props = defineProps<{ name: string | null }>()
const emit = defineEmits<{ close: [] }>()
const { t } = useKernelModulesI18n()
const details = useQuery({
  queryKey: ["kernel-modules", "details", () => props.name],
  queryFn: () => kernelModulesApi.details(props.name!),
  enabled: () => props.name !== null,
  staleTime: 0,
})
</script>

<template>
  <Dialog
    :open="name !== null"
    @update:open="
      (open) => {
        if (!open) emit('close')
      }
    "
  >
    <DialogContent>
      <DialogHeader class="pe-12">
        <DialogTitle class="font-mono break-all"
          ><bdi dir="ltr">{{ name }}</bdi></DialogTitle
        >
        <DialogDescription>{{ t("runtime.description") }}</DialogDescription>
      </DialogHeader>
      <QueryState
        :loading="details.isPending.value"
        :error="details.error.value"
        @retry="details.refetch()"
      >
        <template v-if="details.data.value">
          <KeyValueList
            :rows="[
              {
                label: t('metadata.version'),
                value: details.data.value.version ?? '—',
                mono: true,
              },
              {
                label: t('metadata.srcversion'),
                value: details.data.value.srcversion ?? '—',
                mono: true,
              },
              { label: t('metadata.taint'), value: details.data.value.taint || '—', mono: true },
              {
                label: t('runtime.usedBy'),
                value:
                  [
                    ...new Set([
                      ...details.data.value.module.used_by,
                      ...details.data.value.holders,
                    ]),
                  ]
                    .filter((name) => name !== '[permanent]')
                    .join(', ') || '—',
                mono: true,
              },
            ]"
          />
          <h3 class="mt-4 text-sm font-semibold">{{ t("runtime.parameters") }}</h3>
          <p v-if="!details.data.value.parameters.length" class="text-muted-foreground text-sm">
            {{ t("runtime.parametersEmpty") }}
          </p>
          <dl v-else class="flex flex-col gap-3">
            <div
              v-for="parameter in details.data.value.parameters"
              :key="parameter.name"
              class="min-w-0 font-mono text-sm"
            >
              <dt class="font-semibold break-all">
                <bdi dir="ltr">{{ parameter.name }}</bdi>
              </dt>
              <dd class="text-muted-foreground whitespace-pre-wrap break-all" dir="auto">
                {{ parameter.value ?? t("runtime.unreadable") }}
              </dd>
              <dd v-if="parameter.error" class="text-destructive text-xs break-all" dir="auto">
                {{ parameter.error }}
              </dd>
            </div>
          </dl>
          <p
            v-for="warning in details.data.value.warnings"
            :key="warning"
            class="text-destructive text-xs break-all"
            dir="auto"
          >
            {{ warning }}
          </p>
        </template>
      </QueryState>
    </DialogContent>
  </Dialog>
</template>
