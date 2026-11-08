<script setup lang="ts">
import { computed, ref } from "vue"
import { Plus, Trash2 } from "@lucide/vue"
import { Button } from "@/components/ui/button"
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { useIntegrityI18n } from "../i18n"
import { identityPairs } from "../fields"
import IdentityFields from "./IdentityFields.vue"
import SettingDetails from "./SettingDetails.vue"
import QuickControls from "./QuickControls.vue"

const entries = defineModel<Record<string, string>>({ required: true })
const { t } = useIntegrityI18n()
const name = ref("")
const fields: string[] = identityPairs.flatMap(([field, property]) => [field, property])
const flags = ["spoofBuild", "spoofProps", "spoofProvider", "spoofSignature"] as const
const values = ["spoofVendingFinger", "spoofVendingSdk", "verboseLogs"] as const
const extra = computed(() =>
  Object.keys(entries.value).filter(
    (key) => ![...fields, ...flags, ...values, "fullInjection", "spoofVendingBuild"].includes(key),
  ),
)
const validName = computed(
  () => /^[A-Za-z0-9._*]{1,128}$/.test(name.value) && !(name.value in entries.value),
)
function set(key: string, value: string) {
  entries.value = { ...entries.value, [key]: value }
}
function remove(key: string) {
  const next = { ...entries.value }
  delete next[key]
  entries.value = next
}
function add() {
  if (!validName.value) return
  set(name.value, "")
  name.value = ""
}
</script>

<template>
  <div class="flex flex-col gap-6">
    <p class="text-muted-foreground text-sm">{{ t("fieldsHelp") }}</p>
    <QuickControls v-model="entries" />
    <IdentityFields v-model="entries" />
    <SettingDetails :title="t('editing.advanced')">
      <div class="flex flex-col gap-5">
        <Field orientation="horizontal" class="rounded-xl bg-muted/40 p-3">
          <FieldContent>
            <FieldLabel for="pif-full-injection">{{ t("editing.fullInjection") }}</FieldLabel>
            <FieldDescription>{{ t("flagHelp.fullInjection") }}</FieldDescription>
          </FieldContent>
          <Switch
            id="pif-full-injection"
            :model-value="entries.fullInjection !== '0'"
            @update:model-value="set('fullInjection', $event ? '1' : '0')"
          />
        </Field>
        <Field
          v-for="key in flags"
          :key="key"
          orientation="horizontal"
          class="flex-wrap rounded-xl bg-muted/40 p-3"
        >
          <FieldContent>
            <FieldLabel :for="`pif-${key}`">{{ t(`editing.${key}`) }}</FieldLabel>
            <span class="text-muted-foreground font-mono text-xs">{{ key }}</span>
            <FieldDescription>{{ t(`flagHelp.${key}`) }}</FieldDescription>
          </FieldContent>
          <Select
            :model-value="entries[key] || 'inherit'"
            @update:model-value="$event === 'inherit' ? remove(key) : set(key, String($event))"
          >
            <SelectTrigger :id="`pif-${key}`" class="w-36 shrink-0"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="inherit">{{
                t(
                  key === "spoofSignature" ||
                    (key === "spoofProvider" && entries.fullInjection !== "0")
                    ? "editing.defaultOff"
                    : "editing.defaultOn",
                )
              }}</SelectItem>
              <SelectItem value="0">0 · {{ t("off") }}</SelectItem>
              <SelectItem value="1">1 · {{ t("on") }}</SelectItem>
            </SelectContent>
          </Select>
        </Field>
      </div>
      <div class="grid min-w-0 gap-5 sm:grid-cols-2">
        <Field
          v-for="key in [
            ...values,
            'ro.board.first_api_level',
            ...extra.filter((key) => key !== 'ro.board.first_api_level'),
          ]"
          :key="key"
        >
          <FieldLabel :for="`pif-${key}`" class="break-all font-mono text-xs">{{ key }}</FieldLabel>
          <div class="flex min-w-0 items-center gap-2">
            <Input
              :id="`pif-${key}`"
              :model-value="entries[key] ?? ''"
              class="min-w-0 font-mono text-sm"
              autocapitalize="off"
              spellcheck="false"
              @update:model-value="set(key, String($event))"
            />
            <Button
              v-if="extra.includes(key)"
              variant="ghost"
              size="icon"
              :aria-label="`${t('removeField')}: ${key}`"
              @click="remove(key)"
              ><Trash2
            /></Button>
          </div>
          <FieldDescription
            v-if="
              key === 'spoofVendingFinger' || key === 'spoofVendingSdk' || key === 'verboseLogs'
            "
            >{{ t(`flagHelp.${key}`) }}</FieldDescription
          >
        </Field>
      </div>
      <Field>
        <FieldLabel for="pif-new-field">{{ t("customField") }}</FieldLabel>
        <div class="flex flex-wrap gap-2">
          <Input
            id="pif-new-field"
            v-model="name"
            class="min-w-40 flex-1 font-mono"
            autocapitalize="off"
            spellcheck="false"
            @keydown.enter.prevent="add"
          />
          <Button variant="outline" :disabled="!validName" @click="add"
            ><Plus />{{ t("addField") }}</Button
          >
        </div>
        <FieldDescription>{{ t("wildcardHelp") }}</FieldDescription>
      </Field>
    </SettingDetails>
  </div>
</template>
