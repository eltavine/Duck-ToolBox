<script setup lang="ts">
import { computed, ref } from "vue"
import { useI18n } from "vue-i18n"
import { Check, Search } from "@lucide/vue"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import AppIcon from "@/core/components/AppIcon.vue"
import QueryState from "@/core/components/QueryState.vue"
import VirtualList from "@/core/components/VirtualList.vue"
import { useInstalledApps } from "@/core/installed-apps"
import { useFakerI18n } from "../i18n"
import { existingPackages } from "./selection"

const props = defineProps<{ packages: string }>()
const open = defineModel<boolean>("open", { required: true })
const emit = defineEmits<{ add: [packages: string[]] }>()
const { t } = useFakerI18n()
const { t: coreT } = useI18n()
const apps = useInstalledApps()
const search = ref("")
const selected = ref<string[]>([])
const existing = computed(() => existingPackages(props.packages))
const selectedSet = computed(() => new Set(selected.value))
const rows = computed(() => {
  const query = search.value.trim().toLocaleLowerCase()
  return (apps.data.value ?? []).filter((app) =>
    `${app.label} ${app.packageName}`.toLocaleLowerCase().includes(query),
  )
})
function toggle(packageName: string) {
  selected.value = selectedSet.value.has(packageName)
    ? selected.value.filter((name) => name !== packageName)
    : [...selected.value, packageName]
}
function add() {
  emit(
    "add",
    selected.value.filter((name) => !existing.value.has(name)),
  )
  open.value = false
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent :show-close-button="false" class="p-[16px] sm:p-[24px]">
      <DialogHeader class="pe-0">
        <DialogTitle>{{ t("apps.title") }}</DialogTitle>
        <DialogDescription>{{ t("apps.help") }}</DialogDescription>
      </DialogHeader>
      <div class="relative min-w-0">
        <Search
          class="text-muted-foreground pointer-events-none absolute start-[12px] top-1/2 size-[20px] -translate-y-1/2"
          aria-hidden="true"
        />
        <Input
          v-model="search"
          type="search"
          class="ps-[40px] pe-[12px]"
          :aria-label="t('apps.search')"
          :placeholder="t('apps.search')"
          dir="auto"
          autocomplete="off"
          spellcheck="false"
        />
      </div>
      <p role="status" class="text-muted-foreground text-sm leading-6 tabular-nums">
        {{ t("apps.selected", { count: selected.length }) }}
      </p>
      <QueryState :loading="apps.isPending.value" :error="apps.error.value" @retry="apps.refetch()">
        <VirtualList
          v-if="rows.length"
          :items="rows"
          :item-key="(app) => app.packageName"
          :estimate-size="80"
          class="h-[min(40dvh,24rem)] flex-none rounded-2xl"
          role="group"
          :aria-label="t('apps.title')"
        >
          <template #default="{ item: app }">
            <label
              class="state-layer relative grid min-h-[72px] min-w-0 grid-cols-[minmax(0,1fr)_24px] items-center gap-x-[12px] gap-y-[8px] rounded-xl px-[12px] py-[12px] text-start outline-none focus-within:ring-2 focus-within:ring-inset focus-within:ring-ring @min-[16rem]:grid-cols-[40px_minmax(0,1fr)_24px]"
              :class="[
                existing.has(app.packageName) ? 'cursor-default opacity-60' : 'cursor-pointer',
                selectedSet.has(app.packageName) ? 'bg-primary/12 text-primary' : 'bg-muted/45',
              ]"
            >
              <input
                type="checkbox"
                class="sr-only"
                :checked="existing.has(app.packageName) || selectedSet.has(app.packageName)"
                :disabled="existing.has(app.packageName)"
                @change="toggle(app.packageName)"
              />
              <AppIcon
                :package-name="app.packageName"
                :label="app.label"
                class="col-start-1 row-start-1 size-[40px]"
              />
              <span
                class="col-span-2 col-start-1 row-start-2 min-w-0 @min-[16rem]:col-span-1 @min-[16rem]:col-start-2 @min-[16rem]:row-start-1"
              >
                <span
                  dir="auto"
                  class="block text-sm leading-6 font-medium [overflow-wrap:anywhere]"
                  >{{ app.label }}</span
                >
                <span
                  dir="ltr"
                  class="text-muted-foreground block text-xs leading-5 [overflow-wrap:anywhere]"
                  >{{ app.packageName }}</span
                >
                <span
                  v-if="existing.has(app.packageName)"
                  class="text-muted-foreground block text-xs leading-5"
                  >{{ t("apps.existing") }}</span
                >
              </span>
              <span
                class="col-start-2 row-start-1 grid size-[24px] shrink-0 place-items-center rounded-md border border-input @min-[16rem]:col-start-3"
                :class="
                  (existing.has(app.packageName) || selectedSet.has(app.packageName)) &&
                  'border-primary bg-primary text-primary-foreground'
                "
                aria-hidden="true"
                ><Check
                  v-if="existing.has(app.packageName) || selectedSet.has(app.packageName)"
                  class="size-[18px]"
              /></span>
            </label>
          </template>
        </VirtualList>
        <p v-else class="bg-muted/45 rounded-2xl p-4 text-sm leading-6">
          {{ t("apps.noResults") }}
        </p>
      </QueryState>
      <DialogFooter>
        <Button variant="outline" class="flex-1 basis-[8rem] sm:flex-none" @click="open = false">{{
          coreT("core.actions.cancel")
        }}</Button>
        <Button
          class="flex-1 basis-[8rem] sm:flex-none"
          :disabled="!selected.length"
          @click="add"
          >{{ t("apps.add") }}</Button
        >
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
