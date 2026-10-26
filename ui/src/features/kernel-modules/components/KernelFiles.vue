<script setup lang="ts">
import { useMutation, useQuery } from "@tanstack/vue-query"
import { ChevronDown } from "@lucide/vue"
import { computed, ref, watch } from "vue"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import FilePickerDialog from "@/core/components/FilePickerDialog.vue"
import type { PickedFile } from "@/core/components/FilePickerDialog.vue"
import KeyValueList from "@/core/components/KeyValueList.vue"
import QueryState from "@/core/components/QueryState.vue"
import SectionCard from "@/core/components/SectionCard.vue"
import { formatBytes } from "@/core/format"

import type { ModuleFile } from "../api"
import { kernelModulesApi } from "../api"
import { useKernelModulesI18n } from "../i18n"

const props = defineProps<{
  writable: boolean
  busy: boolean
  loadedNames: string[]
  active: boolean
}>()
const emit = defineEmits<{ load: [file: ModuleFile, parameters: string] }>()
const { t } = useKernelModulesI18n()
const path = ref("")
const search = ref("")
const parameters = ref("")
const picker = ref(false)
const inspected = ref<ModuleFile | null>(null)
const metadataOpen = ref(false)
const catalog = useQuery({
  queryKey: ["kernel-modules", "files"],
  queryFn: kernelModulesApi.files,
  enabled: () => props.active,
})
const files = computed(
  () =>
    catalog.data.value?.files.filter((file) =>
      file.path.toLowerCase().includes(search.value.toLowerCase()),
    ) ?? [],
)
let revision = 0
watch(
  path,
  () => {
    revision++
    inspected.value = null
    metadataOpen.value = false
    parameters.value = ""
  },
  { flush: "sync" },
)
const inspection = useMutation({
  mutationFn: (request: { path: string; revision: number }) =>
    kernelModulesApi.inspect(request.path),
  onSuccess: (file, request) => {
    if (request.revision === revision) inspected.value = file
  },
})
const blocked = computed(() => {
  const file = inspected.value
  if (!file) return null
  if (file.architecture !== "aarch64" || file.elf_class !== 64 || !file.little_endian)
    return "formatUnsupported"
  if (new TextEncoder().encode(file.name).length > 55) return "nameTooLong"
  const magic = file.metadata.vermagic
  if (!magic || magic.length !== 1 || !magic[0]?.trim()) return "vermagicRequired"
  if (props.loadedNames.includes(file.name)) return "alreadyLoaded"
  return null
})
const architectureLabel = computed(() => {
  const architecture = inspected.value?.architecture
  const labels = {
    aarch64: "AArch64",
    arm: "ARM",
    x86_64: "x86-64",
    i386: "x86",
    riscv64: "RISC-V 64",
    riscv32: "RISC-V 32",
    other: t("metadata.other"),
  }
  return architecture ? labels[architecture] : ""
})
const canLoad = computed(
  () =>
    !!inspected.value &&
    props.writable &&
    !props.busy &&
    !inspection.isPending.value &&
    blocked.value === null,
)

function inspect(candidate = path.value) {
  path.value = candidate
  inspected.value = null
  metadataOpen.value = false
  inspection.mutate({ path: candidate, revision })
}
function picked(file: PickedFile) {
  if ("path" in file) inspect(file.path)
}
</script>

<template>
  <div class="flex min-w-0 flex-col gap-6">
    <SectionCard :title="t('files.inspect')" :description="t('files.description')">
      <label for="kernel-module-path" class="mb-2 text-sm font-medium">{{ t("files.path") }}</label>
      <Input
        id="kernel-module-path"
        v-model="path"
        dir="ltr"
        spellcheck="false"
        autocomplete="off"
        autocapitalize="off"
        placeholder="/data/local/tmp/example.ko"
        :disabled="busy || inspection.isPending.value"
        @keydown.enter.prevent="path.trim() && !busy && !inspection.isPending.value && inspect()"
      />
      <div class="mt-3 flex flex-wrap gap-2">
        <Button
          variant="outline"
          :disabled="busy || inspection.isPending.value"
          @click="picker = true"
          >{{ t("files.browse") }}</Button
        >
        <Button :disabled="!path.trim() || busy || inspection.isPending.value" @click="inspect()">{{
          t("files.inspect")
        }}</Button>
      </div>
      <div v-if="inspected" class="mt-4 flex min-w-0 flex-col gap-4">
        <KeyValueList
          :rows="[
            { label: t('metadata.name'), value: inspected.name, mono: true },
            {
              label: t('metadata.architecture'),
              value: `${architectureLabel} · ELF${inspected.elf_class} · ${inspected.little_endian ? 'LE' : 'BE'}`,
              mono: true,
            },
            { label: t('metadata.size'), value: formatBytes(inspected.size) },
            { label: 'SHA-256', value: inspected.sha256, mono: true, copy: true },
          ]"
        />
        <details
          :open="metadataOpen"
          class="group rounded-md border p-3"
          @toggle="metadataOpen = ($event.target as HTMLDetailsElement).open"
        >
          <summary
            class="focus-visible:ring-ring/50 flex min-h-12 cursor-pointer list-none items-center justify-between gap-3 rounded-sm text-sm font-medium outline-none focus-visible:ring-3"
          >
            {{ t("files.metadata")
            }}<ChevronDown
              aria-hidden="true"
              class="size-5 shrink-0 transition-transform group-open:rotate-180"
            />
          </summary>
          <dl v-if="metadataOpen" class="mt-3 flex min-w-0 flex-col gap-4">
            <div v-for="(values, key) in inspected.metadata" :key="key" class="min-w-0 text-sm">
              <dt class="text-muted-foreground">
                <bdi dir="ltr">{{ key }}</bdi>
              </dt>
              <dd class="mt-1 whitespace-pre-wrap break-all">
                <bdi dir="auto">{{ values.join("\n") }}</bdi>
              </dd>
            </div>
          </dl>
        </details>
        <p
          v-for="warning in inspected.warnings"
          :key="warning"
          class="text-muted-foreground text-sm"
        >
          {{ t("files.metadataReplaced") }}
        </p>
        <p
          :class="
            inspected.kernel_release_match === false ? 'text-destructive' : 'text-muted-foreground'
          "
          class="text-sm"
        >
          {{
            t(
              inspected.kernel_release_match === true
                ? "files.matches"
                : inspected.kernel_release_match === false
                  ? "files.mismatch"
                  : "files.unknown",
            )
          }}
        </p>
        <p class="text-muted-foreground text-xs">{{ t("files.compatibility") }}</p>
        <p v-if="blocked" class="text-muted-foreground text-sm">{{ t(`files.${blocked}`) }}</p>
        <label for="kernel-module-parameters" class="text-sm font-medium">{{
          t("files.parameters")
        }}</label>
        <Input
          id="kernel-module-parameters"
          v-model="parameters"
          dir="ltr"
          spellcheck="false"
          autocomplete="off"
          autocapitalize="off"
          placeholder="debug=1"
          :disabled="busy"
          :maxlength="4096"
        />
        <Button :disabled="!canLoad" @click="emit('load', inspected, parameters)">{{
          t("files.load")
        }}</Button>
      </div>
    </SectionCard>
    <SectionCard :title="t('tabs.files')">
      <template #action
        ><Button variant="ghost" :disabled="catalog.isFetching.value" @click="catalog.refetch()">{{
          t("refresh")
        }}</Button></template
      >
      <Input v-model="search" :aria-label="t('search')" :placeholder="t('search')" class="mb-3" />
      <QueryState
        :loading="catalog.isPending.value && active"
        :error="catalog.error.value"
        @retry="catalog.refetch()"
      >
        <p v-if="!files.length" class="text-muted-foreground text-sm">{{ t("files.empty") }}</p>
        <div v-else class="flex max-h-80 flex-col gap-0.5 overflow-y-auto">
          <button
            v-for="file in files"
            :key="file.path"
            class="state-layer focus-visible:ring-ring/50 relative min-h-14 rounded-md py-3 text-start outline-none focus-visible:ring-3"
            :disabled="busy || inspection.isPending.value"
            @click="inspect(file.path)"
          >
            <span class="block font-mono text-sm break-all"
              ><bdi dir="ltr">{{ file.path }}</bdi></span
            >
            <span class="text-muted-foreground text-xs">{{ formatBytes(file.size) }}</span>
          </button>
        </div>
        <p v-if="catalog.data.value?.truncated" class="text-muted-foreground mt-3 text-sm">
          {{ t("files.truncated") }}
        </p>
        <p
          v-for="warning in catalog.data.value?.warnings"
          :key="warning"
          class="text-destructive mt-2 text-xs break-all"
          dir="auto"
        >
          {{ warning }}
        </p>
      </QueryState>
    </SectionCard>
    <FilePickerDialog
      v-model:open="picker"
      extension="ko"
      root="/"
      start="/data/local/tmp"
      :allow-system="false"
      :title="t('files.browse')"
      @pick="picked"
    />
  </div>
</template>
