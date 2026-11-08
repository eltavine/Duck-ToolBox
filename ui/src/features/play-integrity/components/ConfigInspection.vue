<script setup lang="ts">
import { useQuery, useQueryClient } from "@tanstack/vue-query"
import { Button } from "@/components/ui/button"
import QueryState from "@/core/components/QueryState.vue"
import SectionCard from "@/core/components/SectionCard.vue"
import { libraryApi } from "../library-api"
import { useIntegrityI18n } from "../i18n"
import SettingDetails from "./SettingDetails.vue"
const { t } = useIntegrityI18n()
const client = useQueryClient()
const data = useQuery({ queryKey: ["play-integrity.inspection"], queryFn: libraryApi.inspect })
async function refresh() {
  await Promise.all([
    data.refetch(),
    client.invalidateQueries({ queryKey: ["play-integrity.config"] }),
  ])
}
</script>
<template>
  <SectionCard :title="t('inspection.title')" :description="t('inspection.help')">
    <Button variant="secondary" :disabled="data.isFetching.value" @click="refresh()">{{
      t("inspection.refresh")
    }}</Button>
    <QueryState :loading="data.isPending.value" :error="data.error.value" @retry="data.refetch()">
      <p
        v-if="data.data.value?.read_error"
        role="status"
        class="text-muted-foreground mt-3 text-sm"
      >
        {{ t("inspection.unavailable") }}
      </p>
      <p class="text-muted-foreground mt-3 text-sm">
        {{ t(data.data.value?.runtime_observed ? "inspection.observed" : "inspection.unobserved") }}
      </p>
      <p v-if="data.data.value?.runtime_error" role="status" class="mt-2 text-sm">
        {{ t("inspection.runtimeError") }}
      </p>
      <div
        v-for="process in data.data.value?.processes"
        :key="process.process"
        class="mt-3 rounded-xl border p-3 text-sm"
      >
        <p class="break-all font-mono text-xs">{{ process.process }} · PID {{ process.pid }}</p>
        <p v-if="process.observed_at" class="text-muted-foreground mt-1">
          {{ t("guide.checked") }}: {{ new Date(process.observed_at * 1000).toLocaleString() }}
        </p>
        <p class="text-muted-foreground mt-1">
          <template v-if="process.private_requested">
            {{
              t("inspection.private", {
                matched: process.private_matching,
                total: process.private_requested,
              })
            }}
          </template>
          <template v-else>
            {{ t("inspection.hook") }}: {{ t(process.property_hook ? "on" : "off") }}
          </template>
        </p>
      </div>
      <SettingDetails :title="t('inspection.values')" class="mt-3">
        <dl class="flex flex-col gap-4">
          <div
            v-for="row in data.data.value?.rows"
            :key="row.field"
            class="rounded-xl border p-3 text-sm"
          >
            <dt dir="ltr" class="font-mono text-xs break-all">
              {{ row.field }} ↔ {{ row.property }}
            </dt>
            <dd class="mt-3 grid min-w-0 gap-3 sm:grid-cols-3">
              <div
                v-for="[label, value] in [
                  [t('inspection.system'), row.system],
                  ['Build', row.saved_build],
                  [t('editing.property'), row.saved_property],
                ]"
                :key="label ?? ''"
              >
                <p class="text-muted-foreground text-xs">{{ label }}</p>
                <p class="mt-1 font-mono break-all">{{ value || t("inspection.unknown") }}</p>
              </div>
            </dd>
            <dd
              v-for="process in data.data.value?.processes"
              :key="process.process"
              class="mt-3 border-t pt-2"
            >
              <p class="text-muted-foreground break-all font-mono text-xs">{{ process.process }}</p>
              <p class="mt-1 break-all font-mono text-xs">
                Build: {{ process.build[row.field] || t("inspection.unknown") }}
              </p>
              <p class="mt-1 break-all font-mono text-xs">
                Props: {{ process.properties[row.property] || t("inspection.unknown") }}
              </p>
            </dd>
          </div>
        </dl>
      </SettingDetails>
    </QueryState>
  </SectionCard>
</template>
