<script setup lang="ts">
import { computed, ref } from "vue"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Input } from "@/components/ui/input"
import { useIntegrityI18n } from "../i18n"
const entries = defineModel<Record<string, string>>({ required: true })
const { t } = useIntegrityI18n()
function set(key: string, value: string) {
  entries.value = { ...entries.value, [key]: value }
}
const customEditing = ref(false)
const sdkEditing = ref(false)
const mode = computed(() => {
  const full = entries.value.fullInjection !== "0"
  const build = entries.value.spoofVendingBuild
  if (full && build === "1") return "build"
  if (sdkEditing.value || Number(entries.value.spoofVendingSdk) > 0) return "sdk"
  const finger = entries.value.spoofVendingFinger
  if (customEditing.value || (finger && finger !== "0" && finger !== "1")) return "custom"
  if (finger === "1") return "fingerprint"
  return full && build !== "0" ? "build" : "off"
})
function vending(value: unknown) {
  const selected = String(value)
  const existingCustom =
    mode.value === "custom" ? entries.value.spoofVendingFinger : entries.value.FINGERPRINT
  customEditing.value = selected === "custom"
  sdkEditing.value = false
  entries.value = {
    ...entries.value,
    spoofVendingBuild: selected === "build" ? "1" : "0",
    spoofVendingSdk: selected === "sdk" ? "1" : "0",
    spoofVendingFinger:
      selected === "fingerprint" ? "1" : selected === "custom" ? (existingCustom ?? "") : "0",
  }
}
function all() {
  customEditing.value = false
  sdkEditing.value = false
  entries.value = {
    ...entries.value,
    fullInjection: "1",
    spoofBuild: "1",
    spoofProps: "1",
    spoofVendingBuild: "1",
    spoofVendingFinger: "0",
    spoofVendingSdk: "0",
  }
}
</script>
<template>
  <div class="flex flex-col gap-4 rounded-xl border p-4">
    <p class="font-medium">{{ t("quick.title") }}</p>
    <Field v-for="key in ['spoofBuild', 'spoofProps'] as const" :key="key" orientation="horizontal">
      <FieldContent
        ><FieldLabel :for="`quick-${key}`">{{ t(`editing.${key}`) }}</FieldLabel
        ><FieldDescription>{{ t(`flagHelp.${key}`) }}</FieldDescription></FieldContent
      >
      <Switch
        :id="`quick-${key}`"
        :model-value="entries[key] !== '0'"
        @update:model-value="set(key, $event ? '1' : '0')"
      />
    </Field>
    <Field
      ><FieldLabel for="quick-vending">{{ t("quick.vending") }}</FieldLabel>
      <Select :model-value="mode" @update:model-value="vending"
        ><SelectTrigger id="quick-vending" class="w-full"><SelectValue /></SelectTrigger>
        <SelectContent
          ><SelectItem
            v-for="value in ['off', 'build', 'fingerprint', 'custom', 'sdk']"
            :key="value"
            :value="value"
            :disabled="value === 'build' && entries.fullInjection === '0'"
            >{{ t(`quick.${value}`) }}</SelectItem
          ></SelectContent
        >
      </Select>
      <FieldDescription>{{ t("quick.vendingHelp") }}</FieldDescription>
    </Field>
    <Field v-if="mode === 'custom'"
      ><FieldLabel for="quick-fingerprint">{{ t("quick.custom") }}</FieldLabel
      ><Input
        id="quick-fingerprint"
        :model-value="entries.spoofVendingFinger"
        @focus="customEditing = true"
        @blur="customEditing = false"
        @update:model-value="set('spoofVendingFinger', String($event))"
    /></Field>
    <Field v-if="mode === 'sdk'"
      ><FieldLabel for="quick-sdk">{{ t("quick.sdk") }}</FieldLabel
      ><Input
        id="quick-sdk"
        :model-value="entries.spoofVendingSdk"
        @focus="sdkEditing = true"
        @blur="sdkEditing = false"
        @update:model-value="set('spoofVendingSdk', String($event))"
      /><FieldDescription>{{ t("flagHelp.spoofVendingSdk") }}</FieldDescription></Field
    >
    <Button variant="secondary" class="self-start" @click="all">{{ t("quick.all") }}</Button>
    <p class="text-muted-foreground text-sm">{{ t("quick.draft") }}</p>
  </div>
</template>
