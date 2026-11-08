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
import { existingTargets } from "./selection"

const props = defineProps<{ packages: string }>()
const open = defineModel<boolean>("open", { required: true })
const emit = defineEmits<{ add: [packages: string[]] }>()
const { t } = useFakerI18n()
const { t: coreT } = useI18n()
const apps = useInstalledApps("user")
const search = ref("")
const selected = ref<string[]>([])
const existing = computed(() => existingTargets(props.packages))
const selectedSet = computed(() => new Set(selected.value))
const rows = computed(() => {
  const query = search.value.trim().toLocaleLowerCase()
  return (apps.data.value ?? [])
    .map((app) => ({
      ...app,
      target: app.userId === undefined ? app.packageName : `${app.packageName}@${app.userId}`,
    }))
    .filter((app) => `${app.label} ${app.target}`.toLocaleLowerCase().includes(query))
})
function toggle(target: string) {
  selected.value = selectedSet.value.has(target)
    ? selected.value.filter((key) => key !== target)
    : [...selected.value, target]
}
function add() {
  emit(
    "add",
    selected.value.filter((key) => !existing.value(key)),
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
          :item-key="(app) => app.target"
          :estimate-size="80"
          class="h-[min(40dvh,24rem)] flex-none rounded-2xl"
          role="group"
          :aria-label="t('apps.title')"
        >
          <template #default="{ item: app }">
            <label
              class="state-layer relative grid min-h-[72px] min-w-0 grid-cols-[minmax(0,1fr)_24px] items-center gap-x-[12px] gap-y-[8px] rounded-xl px-[12px] py-[12px] text-start outline-none focus-within:ring-2 focus-within:ring-inset focus-within:ring-ring @min-[16rem]:grid-cols-[40px_minmax(0,1fr)_24px]"
              :class="[
                existing(app.target) ? 'cursor-default opacity-60' : 'cursor-pointer',
                selectedSet.has(app.target) ? 'bg-primary/12 text-primary' : 'bg-muted/45',
              ]"
            >
              <input
                type="checkbox"
                class="sr-only"
                :checked="existing(app.target) || selectedSet.has(app.target)"
                :disabled="existing(app.target)"
                @change="toggle(app.target)"
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
                  >{{ app.target }}</span
                >
                <span
                  v-if="existing(app.target)"
                  class="text-muted-foreground block text-xs leading-5"
                  >{{ t("apps.existing") }}</span
                >
              </span>
              <span
                class="col-start-2 row-start-1 grid size-[24px] shrink-0 place-items-center rounded-md border border-input @min-[16rem]:col-start-3"
                :class="
                  (existing(app.target) || selectedSet.has(app.target)) &&
                  'border-primary bg-primary text-primary-foreground'
                "
                aria-hidden="true"
                ><Check
                  v-if="existing(app.target) || selectedSet.has(app.target)"
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
