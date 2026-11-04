<script setup lang="ts">
import { ChevronDown } from "@lucide/vue"
import { computed, ref } from "vue"

import { Card, CardContent } from "@/components/ui/card"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

import type { Draft } from "../draft"
import { useRkpI18n } from "../i18n"

const draft = defineModel<Draft>({ required: true })
const { t } = useRkpI18n()
const open = ref(false)

type DeviceKey = keyof Draft["device"]
const deviceFields = computed<
  { key: DeviceKey; label: string; numeric?: boolean; mono?: boolean }[]
>(() => [
  { key: "brand", label: t("profile.brand") },
  { key: "model", label: t("profile.model") },
  { key: "device", label: t("profile.deviceCode") },
  { key: "product", label: t("profile.product") },
  { key: "manufacturer", label: t("profile.manufacturer") },
  { key: "os_version", label: t("profile.osVersion") },
  { key: "fused", label: t("profile.fused"), numeric: true },
  { key: "boot_patch_level", label: t("profile.bootPatch"), numeric: true },
  { key: "system_patch_level", label: t("profile.systemPatch"), numeric: true },
  { key: "vendor_patch_level", label: t("profile.vendorPatch"), numeric: true },
  { key: "dice_issuer", label: t("profile.diceIssuer") },
  { key: "dice_subject", label: t("profile.diceSubject") },
])

function setDevice(key: DeviceKey, value: string | number, numeric?: boolean) {
  const record = draft.value.device as Record<DeviceKey, string | number>
  record[key] = numeric ? Number(value) || 0 : String(value)
}
</script>

<template>
  <Card class="py-0">
    <Collapsible v-model:open="open">
      <CollapsibleTrigger
        class="state-layer focus-visible:ring-ring/50 relative flex min-h-18 w-full items-center justify-between gap-4 rounded-lg p-4 text-start outline-none focus-visible:ring-3"
      >
        <span class="flex flex-col gap-1">
          <span class="text-base">{{ t("profile.advanced") }}</span>
          <span class="text-muted-foreground text-sm">{{ t("profile.advancedHint") }}</span>
        </span>
        <ChevronDown
          :class="[
            'text-muted-foreground size-5 shrink-0 transition-transform duration-350 ease-spring-fast',
            open ? 'rotate-180' : '',
          ]"
        />
      </CollapsibleTrigger>
      <CollapsibleContent>
        <CardContent class="flex flex-col gap-5 px-4 pt-1 pb-4">
          <Field>
            <FieldLabel for="fingerprint">{{ t("profile.fingerprint") }}</FieldLabel>
            <Input id="fingerprint" v-model="draft.fingerprint" class="font-mono text-sm" />
          </Field>
          <Field>
            <div class="flex items-center justify-between gap-4">
              <FieldLabel for="include-fingerprint">{{
                t("profile.includeFingerprint")
              }}</FieldLabel>
              <Switch id="include-fingerprint" v-model="draft.include_fingerprint" />
            </div>
            <FieldDescription>{{ t("profile.includeFingerprintHint") }}</FieldDescription>
          </Field>
          <Field>
            <FieldLabel for="server-url">{{ t("profile.serverUrl") }}</FieldLabel>
            <Input
              id="server-url"
              v-model="draft.server_url"
              class="font-mono text-sm"
              inputmode="url"
            />
          </Field>
          <Field>
            <FieldLabel for="output-path">{{ t("profile.outputPath") }}</FieldLabel>
            <Input id="output-path" v-model="draft.output_path" class="font-mono text-sm" />
          </Field>
          <Field>
            <FieldLabel for="vbmeta">{{ t("profile.vbmetaDigest") }}</FieldLabel>
            <Input id="vbmeta" v-model="draft.device.vbmeta_digest" class="font-mono text-sm" />
          </Field>
          <div
            class="grid grid-cols-[repeat(auto-fit,minmax(min(100%,12rem),1fr))] gap-x-3 gap-y-5"
          >
            <Field v-for="field in deviceFields" :key="field.key">
              <FieldLabel :for="`device-${field.key}`">{{ field.label }}</FieldLabel>
              <Input
                :id="`device-${field.key}`"
                :model-value="draft.device[field.key]"
                :type="field.numeric ? 'number' : 'text'"
                :inputmode="field.numeric ? 'numeric' : undefined"
                @update:model-value="setDevice(field.key, $event, field.numeric)"
              />
            </Field>
          </div>
        </CardContent>
      </CollapsibleContent>
    </Collapsible>
  </Card>
</template>
