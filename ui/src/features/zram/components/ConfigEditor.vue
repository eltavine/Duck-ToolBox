<script setup lang="ts">
import { computed, ref } from "vue"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import type { ZramConfig, ZramDevice, ZramProfile } from "../api"
import { useZramI18n } from "../i18n"
import { parsePlan } from "../input"
const props = defineProps<{
  device: ZramDevice
  profile?: ZramProfile
  total: number
  writable: boolean
  busy: boolean
  canRestore: boolean
}>()
const emit = defineEmits<{
  request: [kind: "apply" | "save", config: ZramConfig, boot: boolean]
  restore: []
  forget: []
}>()
const { t, locale } = useZramI18n()
const initial = props.profile?.config
const size = ref(String(Math.round((initial?.size_bytes ?? props.device.size) / 1048576)))
const algorithm = ref(
  initial?.algorithm ?? props.device.algorithm ?? props.device.algorithms[0] ?? "",
)
const priority = ref(
  String(
    initial
      ? (initial.priority ?? -1)
      : props.device.swap && props.device.swap.priority >= 0
        ? props.device.swap.priority
        : -1,
  ),
)
const boot = ref(props.profile?.apply_at_boot ?? false)
const plan = computed(() =>
  !props.device.algorithms.includes(algorithm.value)
    ? null
    : parsePlan(
        String(size.value),
        String(priority.value),
        algorithm.value,
        props.device.stats.memory_limit ?? 0,
        props.total,
      ),
)
const range = computed(() =>
  new Intl.NumberFormat(locale.value).format(Math.floor((props.total * 2) / 1048576)),
)
</script>
<template>
  <div class="flex min-w-0 flex-col gap-4">
    <p class="text-muted-foreground text-sm">{{ t("planHelp") }}</p>
    <div class="grid min-w-0 gap-4 sm:grid-cols-2">
      <div class="min-w-0 space-y-2">
        <Label :for="`zram-size-${device.name}`">{{ t("sizeInput") }}</Label>
        <Input
          :id="`zram-size-${device.name}`"
          v-model="size"
          inputmode="numeric"
          dir="ltr"
          :aria-invalid="!plan"
          :aria-describedby="`zram-input-help-${device.name}`"
          :disabled="busy || !writable"
        />
      </div>
      <div class="min-w-0 space-y-2">
        <Label :for="`zram-algorithm-${device.name}`">{{ t("algorithm") }}</Label>
        <Select v-model="algorithm" :disabled="busy || !writable">
          <SelectTrigger :id="`zram-algorithm-${device.name}`" class="w-full"
            ><SelectValue
          /></SelectTrigger>
          <SelectContent
            ><SelectItem v-for="item in device.algorithms" :key="item" :value="item"
              ><bdi dir="ltr">{{ item }}</bdi></SelectItem
            ></SelectContent
          >
        </Select>
      </div>
      <div class="min-w-0 space-y-2">
        <Label :for="`zram-priority-${device.name}`">{{ t("priority") }}</Label>
        <Input
          :id="`zram-priority-${device.name}`"
          v-model="priority"
          inputmode="numeric"
          dir="ltr"
          :aria-invalid="!plan"
          :aria-describedby="`zram-input-help-${device.name}`"
          :disabled="busy || !writable"
        />
      </div>
    </div>
    <p
      :id="`zram-input-help-${device.name}`"
      :class="['text-sm', plan ? 'text-muted-foreground' : 'text-destructive']"
    >
      {{ t("inputHelp", { max: range }) }}
    </p>
    <label class="flex min-h-12 cursor-pointer items-center justify-between gap-4 rounded-md py-2">
      <span class="min-w-0"
        ><span class="block text-sm font-medium">{{ t("boot") }}</span
        ><span class="text-muted-foreground mt-1 block text-xs">{{ t("bootHelp") }}</span></span
      >
      <Switch v-model="boot" :aria-label="t('boot')" :disabled="busy || !writable" />
    </label>
    <div class="flex flex-wrap gap-2">
      <Button
        class="min-h-12"
        :disabled="!writable || busy || !plan"
        @click="plan && emit('request', 'apply', plan, boot)"
        >{{ t("apply") }}</Button
      >
      <Button
        variant="outline"
        class="min-h-12"
        :disabled="!writable || busy || !plan"
        @click="plan && emit('request', 'save', plan, boot)"
        >{{ t("save") }}</Button
      >
      <Button
        variant="outline"
        class="min-h-12"
        :disabled="!writable || busy || !canRestore"
        @click="emit('restore')"
        >{{ t("restore") }}</Button
      >
      <Button
        v-if="profile"
        variant="ghost"
        class="min-h-12"
        :disabled="busy"
        @click="emit('forget')"
        >{{ t("forget") }}</Button
      >
    </div>
    <p v-if="profile" class="text-muted-foreground text-xs">
      {{ t(profile.apply_at_boot ? "bootEnabled" : "bootDisabled") }}
    </p>
  </div>
</template>
