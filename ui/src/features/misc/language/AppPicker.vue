<script setup lang="ts">
import { useQuery } from "@tanstack/vue-query"
import { computed, ref } from "vue"
import { useI18n } from "vue-i18n"
import { Input } from "@/components/ui/input"
import { bridge } from "@/core/bridge"
import AppIcon from "@/core/components/AppIcon.vue"
import QueryState from "@/core/components/QueryState.vue"
import VirtualList from "@/core/components/VirtualList.vue"
import { useMiscI18n } from "../i18n"

const emit = defineEmits<{ select: [packageName: string] }>()
const { t } = useMiscI18n()
const { t: coreT } = useI18n()
const search = ref("")
const apps = useQuery({
  queryKey: ["misc", "installed-apps"],
  queryFn: async ({ signal }) => {
    const names = await bridge().listPackages("all")
    const metadata = await bridge().packagesInfo(names, signal)
    const labels = new Map(metadata.map((app) => [app.packageName, app.label]))
    return names
      .map((packageName) => ({ packageName, label: labels.get(packageName) ?? packageName }))
      .sort((a, b) => a.label.localeCompare(b.label))
  },
})
const rows = computed(() => {
  const query = search.value.trim().toLocaleLowerCase()
  return (apps.data.value ?? []).filter((app) =>
    `${app.label} ${app.packageName}`.toLocaleLowerCase().includes(query),
  )
})
</script>

<template>
  <div class="bg-muted/40 flex min-w-0 flex-col gap-3 rounded-xl p-3">
    <Input
      v-model="search"
      type="search"
      :aria-label="coreT('core.actions.search')"
      :placeholder="coreT('core.actions.search')"
    />
    <QueryState :loading="apps.isPending.value" :error="apps.error.value" @retry="apps.refetch()">
      <VirtualList
        :items="rows"
        :item-key="(app) => app.packageName"
        :estimate-size="72"
        class="max-h-64"
      >
        <template #default="{ item: app }">
          <button
            type="button"
            class="hover:bg-muted flex min-h-18 w-full items-center gap-3 rounded-lg p-2 text-start"
            @click="emit('select', app.packageName)"
          >
            <AppIcon :package-name="app.packageName" :label="app.label" />
            <span class="min-w-0 flex-1">
              <span class="block break-words text-sm font-medium">{{ app.label }}</span>
              <span dir="ltr" class="text-muted-foreground block break-all text-xs">{{
                app.packageName
              }}</span>
            </span>
          </button>
        </template>
      </VirtualList>
      <p v-if="!rows.length" class="text-muted-foreground py-3 text-sm">
        {{ t("language.noApps") }}
      </p>
    </QueryState>
  </div>
</template>
