<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { configSchema } from "../api"
import type { Config } from "../api"
import { useLinuxI18n } from "../i18n"
const props = defineProps<{ id: string; revision: string; config: Config; disabled: boolean }>()
const emit = defineEmits<{ save: [config: Config] }>()
const { t } = useLinuxI18n()
const editor = ref<HTMLElement | null>(null)
const baseline = ref({ revision: props.revision, config: configSchema.parse(props.config) })
const draft = ref<Config>(configSchema.parse(baseline.value.config))
const dns = ref(draft.value.dns.join("\n"))
const parsed = computed(() =>
  configSchema.safeParse({ ...draft.value, dns: dns.value.split(/\s+/).filter(Boolean) }),
)
const dirty = computed(
  () =>
    !parsed.value.success ||
    JSON.stringify(parsed.value.data) !== JSON.stringify(baseline.value.config),
)
const changed = computed(() => props.revision !== baseline.value.revision)
function reload(focus = false) {
  baseline.value = { revision: props.revision, config: configSchema.parse(props.config) }
  draft.value = configSchema.parse(props.config)
  dns.value = draft.value.dns.join("\n")
  if (focus)
    void nextTick(() => editor.value?.querySelector("input")?.focus({ preventScroll: true }))
}
watch(
  () => props.revision,
  () => {
    if (
      !dirty.value ||
      (parsed.value.success &&
        JSON.stringify(parsed.value.data) === JSON.stringify(configSchema.parse(props.config)))
    )
      reload()
  },
)
function invalid(path: (string | number)[]) {
  return (
    !parsed.value.success &&
    parsed.value.error.issues.some((issue) => path.every((key, index) => issue.path[index] === key))
  )
}
function addService() {
  const used = new Set(draft.value.services.map((service) => service.id))
  let index = 1
  while (used.has(`service-${index}`)) index++
  draft.value.services.push({ id: `service-${index}`, command: "", enabled: true })
}
function save() {
  if (parsed.value.success && !changed.value) emit("save", parsed.value.data)
}
</script>
<template>
  <div ref="editor" class="flex min-w-0 flex-col gap-4">
    <p class="text-muted-foreground text-sm">{{ t("configHelp") }}</p>
    <div v-if="changed" class="space-y-3" role="alert">
      <p class="text-sm">{{ t("configChanged") }}</p>
      <Button variant="outline" class="min-h-12" :disabled="disabled" @click="reload(true)">{{
        t("reloadConfig")
      }}</Button>
    </div>
    <div class="space-y-2">
      <Label :for="`linux-config-name-${id}`">{{ t("name") }}</Label
      ><Input
        :id="`linux-config-name-${id}`"
        v-model="draft.name"
        :disabled="disabled"
        :aria-describedby="`linux-config-name-help-${id}`"
        :aria-invalid="invalid(['name'])"
      />
      <p :id="`linux-config-name-help-${id}`" class="text-muted-foreground text-xs">
        {{ t("nameHelp") }}
      </p>
    </div>
    <label class="flex min-h-12 flex-wrap items-center justify-between gap-3"
      ><span class="min-w-0 flex-[1_1_12rem]">{{ t("boot") }}</span
      ><Switch v-model="draft.auto_start" :disabled="disabled" :aria-label="t('boot')"
    /></label>
    <div class="space-y-2">
      <Label :for="`linux-dns-${id}`">{{ t("dns") }}</Label
      ><Textarea
        :id="`linux-dns-${id}`"
        v-model="dns"
        dir="ltr"
        :disabled="disabled"
        :aria-describedby="`linux-dns-help-${id}`"
        :aria-invalid="invalid(['dns'])"
      />
      <p :id="`linux-dns-help-${id}`" class="text-muted-foreground text-xs">{{ t("dnsHelp") }}</p>
    </div>
    <p class="text-muted-foreground text-sm">{{ t("serviceHelp") }}</p>
    <div
      v-for="(service, index) in draft.services"
      :key="index"
      class="border-border min-w-0 space-y-3 rounded-lg border p-4"
    >
      <div class="space-y-2">
        <Label :for="`linux-service-${id}-${index}`">{{ t("id") }}</Label
        ><Input
          :id="`linux-service-${id}-${index}`"
          v-model="service.id"
          dir="ltr"
          :disabled="disabled"
          :aria-invalid="invalid(['services', index, 'id'])"
          :aria-describedby="`linux-service-help-${id}-${index}`"
        />
        <p :id="`linux-service-help-${id}-${index}`" class="text-muted-foreground text-xs">
          {{ t("serviceIdHelp") }}
        </p>
      </div>
      <div class="space-y-2">
        <Label :for="`linux-command-${id}-${index}`">{{ t("command") }}</Label
        ><Textarea
          :id="`linux-command-${id}-${index}`"
          v-model="service.command"
          dir="ltr"
          class="font-mono"
          :disabled="disabled"
          :aria-invalid="invalid(['services', index, 'command'])"
          :aria-describedby="`linux-command-help-${id}-${index}`"
        />
        <p :id="`linux-command-help-${id}-${index}`" class="text-muted-foreground text-xs">
          {{ t("commandHelp") }}
        </p>
      </div>
      <div class="flex flex-wrap items-center gap-3">
        <label class="inline-flex min-h-12 items-center gap-2"
          ><Switch
            v-model="service.enabled"
            :disabled="disabled"
            :aria-label="`${t('enabled')} ${service.id}`"
          /><span>{{ t("enabled") }}</span></label
        ><Button
          variant="outline"
          class="min-h-12"
          :disabled="disabled"
          @click="draft.services.splice(index, 1)"
          >{{ t("removeService") }}</Button
        >
      </div>
    </div>
    <Button
      variant="outline"
      class="min-h-12 self-start"
      :disabled="disabled || draft.services.length >= 8"
      @click="addService"
      >{{ t("addService") }}</Button
    >
    <p v-if="!parsed.success" role="status" class="text-destructive text-sm">
      {{ t("invalidConfig") }}
    </p>
    <Button
      class="min-h-12 self-start"
      :disabled="disabled || changed || !parsed.success"
      @click="save"
      >{{ t("save") }}</Button
    >
  </div>
</template>
