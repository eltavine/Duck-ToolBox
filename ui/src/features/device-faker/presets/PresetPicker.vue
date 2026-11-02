<script setup lang="ts">
import { computed, ref, watch } from "vue"
import { Check, ChevronDown, ChevronRight, Search } from "@lucide/vue"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { openExternal } from "@/core/links"
import { useFakerI18n } from "../i18n"
import {
  devicePresets,
  findPresets,
  fingerprintConflicts,
  presetBrands,
  presetFields,
  presetSource,
  vendorDisplayName,
  type DevicePreset,
} from "./catalog"

const emit = defineEmits<{ select: [preset: DevicePreset] }>()
const { t } = useFakerI18n()
const query = ref("")
const brand = ref("all")
const limit = ref(8)
const selected = ref<DevicePreset>()
const panel = ref<HTMLDetailsElement>()
const results = computed(() => findPresets(query.value, brand.value === "all" ? "" : brand.value))
const conflicts = computed(() => (selected.value ? fingerprintConflicts(selected.value) : []))
watch([query, brand], () => {
  limit.value = 8
  selected.value = undefined
})
function useSelected() {
  if (!selected.value) return
  emit("select", selected.value)
  if (panel.value) panel.value.open = false
}
</script>
<template>
  <details ref="panel" class="group/presets min-w-0 rounded-2xl bg-muted/45 p-[12px]">
    <summary
      class="flex min-h-12 cursor-pointer items-center justify-between gap-[12px] rounded-xl text-sm font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <span>{{ t("presets.title") }}</span>
      <span class="text-muted-foreground flex shrink-0 items-center gap-[8px] text-xs tabular-nums">
        {{ devicePresets.length }}
        <ChevronDown
          class="size-[20px] transition-transform group-open/presets:rotate-180 motion-reduce:transition-none"
          aria-hidden="true"
        />
      </span>
    </summary>
    <div class="mt-3 flex min-w-0 flex-col gap-4">
      <p class="text-muted-foreground text-sm leading-6">{{ t("presets.help") }}</p>
      <div class="grid min-w-0 gap-3 sm:grid-cols-[minmax(0,1fr)_12rem]">
        <div class="min-w-0">
          <label for="faker-preset-search" class="text-sm font-medium">{{
            t("presets.search")
          }}</label>
          <div class="relative mt-2">
            <Search
              class="text-muted-foreground pointer-events-none absolute start-[12px] top-1/2 size-[20px] -translate-y-1/2"
              aria-hidden="true"
            />
            <Input
              id="faker-preset-search"
              v-model="query"
              type="search"
              class="ps-[40px] pe-[12px]"
              dir="auto"
              autocomplete="off"
              spellcheck="false"
            />
          </div>
        </div>
        <div class="min-w-0">
          <label for="faker-preset-brand" class="text-sm font-medium">{{
            t("presets.brand")
          }}</label>
          <Select v-model="brand">
            <SelectTrigger id="faker-preset-brand" class="mt-2 w-full px-[12px]"
              ><SelectValue
            /></SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{{ t("presets.allBrands") }}</SelectItem>
              <SelectItem v-for="[id, label] in presetBrands" :key="id" :value="id"
                ><bdi>{{ label }}</bdi></SelectItem
              >
            </SelectContent>
          </Select>
        </div>
      </div>
      <p role="status" class="text-muted-foreground text-xs leading-5">
        {{ t("presets.results", { count: results.length }) }}
      </p>
      <ul
        v-if="results.length"
        :aria-label="t('presets.resultsLabel')"
        class="flex min-w-0 flex-col gap-1"
      >
        <li v-for="preset in results.slice(0, limit)" :key="preset.id" class="min-w-0">
          <button
            type="button"
            :aria-pressed="selected?.id === preset.id"
            class="state-layer flex min-h-16 w-full min-w-0 items-center gap-[12px] rounded-xl px-[12px] py-[12px] text-start outline-none focus-visible:ring-2 focus-visible:ring-ring"
            :class="
              selected?.id === preset.id
                ? 'bg-primary/12 text-primary'
                : 'bg-card text-card-foreground'
            "
            @click="selected = preset"
          >
            <span class="min-w-0 flex-1">
              <span
                class="block text-sm leading-6 font-medium [overflow-wrap:anywhere] rtl:text-right"
                dir="auto"
                >{{ preset.label }}</span
              >
              <span class="text-muted-foreground block text-xs leading-5 [overflow-wrap:anywhere]"
                ><bdi
                  >{{ vendorDisplayName(preset.fields.brand ?? "") }} ·
                  {{ preset.fields.model }}</bdi
                ></span
              >
            </span>
            <Check
              v-if="selected?.id === preset.id"
              class="size-[20px] shrink-0"
              aria-hidden="true"
            />
            <ChevronRight
              v-else
              class="text-muted-foreground size-[20px] shrink-0 rtl:rotate-180"
              aria-hidden="true"
            />
          </button>
        </li>
      </ul>
      <p v-else class="bg-card rounded-xl p-4 text-sm leading-6">{{ t("presets.noResults") }}</p>
      <Button v-if="results.length > limit" variant="ghost" @click="limit += 8">{{
        t("presets.showMore")
      }}</Button>
      <section
        v-if="selected"
        class="bg-primary/8 flex min-w-0 flex-col gap-3 rounded-2xl p-[12px]"
        :aria-label="t('presets.selected')"
      >
        <p
          class="text-sm leading-6 font-semibold [overflow-wrap:anywhere] rtl:text-right"
          dir="auto"
        >
          {{ selected.label }}
        </p>
        <p class="text-muted-foreground text-sm leading-6">
          {{ t(selected.fields.fingerprint ? "presets.withFingerprint" : "presets.identityOnly") }}
        </p>
        <p v-if="conflicts.length" role="status" class="text-sm leading-6">
          {{
            t("presets.conflict", { fields: conflicts.map((key) => t(`fields.${key}`)).join(", ") })
          }}
        </p>
        <details>
          <summary
            class="flex min-h-12 cursor-pointer items-center text-sm font-medium underline outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            {{ t("presets.details") }}
          </summary>
          <dl class="mt-2 grid min-w-0 gap-3">
            <div v-for="key in presetFields" :key="key" class="min-w-0">
              <dt class="text-muted-foreground text-xs leading-5">{{ t(`fields.${key}`) }}</dt>
              <dd
                class="text-sm leading-6 [overflow-wrap:anywhere] rtl:text-right"
                :dir="
                  !selected.fields[key] ||
                  ['manufacturer', 'brand', 'model', 'marketname'].includes(key)
                    ? 'auto'
                    : 'ltr'
                "
              >
                {{ selected.fields[key] || t("presets.notSupplied") }}
              </dd>
            </div>
          </dl>
        </details>
        <Button class="px-[16px]" @click="useSelected">{{ t("presets.use") }}</Button>
      </section>
      <a
        :href="presetSource.issue"
        class="text-muted-foreground flex min-h-12 items-center text-sm underline"
        @click.prevent="openExternal(presetSource.issue)"
        >{{ t("presets.source", { author: `\u2068${presetSource.contributor}\u2069` }) }}</a
      >
    </div>
  </details>
</template>
