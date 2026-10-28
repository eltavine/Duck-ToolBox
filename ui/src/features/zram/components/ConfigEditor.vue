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
import { parsePlan, capacityBytes, swapPriority } from "../input"
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
const size = ref(String((initial?.size_bytes ?? props.device.size) / 1048576))
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
const previousPriority = ref(
  initial?.priority != null
    ? String(initial.priority)
    : props.device.swap && props.device.swap.priority >= 0
      ? String(props.device.swap.priority)
      : "100",
)
const automaticPriority = computed({
  get: () => swapPriority(String(priority.value)) === null,
  set: (enabled: boolean) => {
    priority.value = enabled ? "-1" : previousPriority.value
  },
})
function changePriority(value: string | number) {
  priority.value = String(value)
  const explicit = swapPriority(priority.value)
  if (typeof explicit === "number") previousPriority.value = String(explicit)
}
const boot = ref(props.profile?.apply_at_boot ?? false)
const sizeInvalid = computed(() => capacityBytes(String(size.value), props.total) === null)
const priorityInvalid = computed(() => swapPriority(String(priority.value)) === undefined)
const algorithmInvalid = computed(() => !props.device.algorithms.includes(algorithm.value))
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
  <div class="@container flex min-w-0 flex-col gap-4">
    <p class="text-muted-foreground text-sm">{{ t("planHelp") }}</p>
    <div class="grid min-w-0 gap-4 sm:grid-cols-2">
      <div class="min-w-0 space-y-2">
        <Label :for="`zram-size-${device.name}`">{{ t("sizeInput") }}</Label>
        <Input
          :id="`zram-size-${device.name}`"
          v-model="size"
          inputmode="numeric"
          dir="ltr"
          :aria-invalid="sizeInvalid"
          :aria-describedby="`zram-input-help-${device.name}${sizeInvalid ? ` zram-size-error-${device.name}` : ''}`"
          :disabled="busy || !writable"
        />
        <p
          v-if="sizeInvalid"
          :id="`zram-size-error-${device.name}`"
          class="text-destructive text-sm"
          aria-live="polite"
        >
          {{ t("sizeError", { max: range }) }}
        </p>
      </div>
      <div class="min-w-0 space-y-2">
        <Label :for="`zram-algorithm-${device.name}`">{{ t("algorithm") }}</Label>
        <Select v-model="algorithm" :disabled="busy || !writable">
          <SelectTrigger
            :id="`zram-algorithm-${device.name}`"
            class="w-full"
            :aria-invalid="algorithmInvalid"
            :aria-describedby="algorithmInvalid ? `zram-algorithm-error-${device.name}` : undefined"
            ><SelectValue
              ><bdi dir="ltr">{{ algorithm }}</bdi></SelectValue
            ></SelectTrigger
          >
          <SelectContent
            ><SelectItem v-for="item in device.algorithms" :key="item" :value="item"
              ><bdi dir="ltr">{{ item }}</bdi></SelectItem
            ></SelectContent
          >
        </Select>
        <p
          v-if="algorithmInvalid"
          :id="`zram-algorithm-error-${device.name}`"
          class="text-destructive text-sm"
          aria-live="polite"
        >
          {{ t("algorithmError") }}
        </p>
      </div>
      <div class="min-w-0 space-y-2">
        <Label :for="`zram-priority-${device.name}`">{{ t("priority") }}</Label>
        <Input
          :id="`zram-priority-${device.name}`"
          :model-value="priority"
          inputmode="numeric"
          dir="ltr"
          :aria-invalid="priorityInvalid"
          :aria-describedby="`zram-input-help-${device.name}${priorityInvalid ? ` zram-priority-error-${device.name}` : ''}`"
          :readonly="automaticPriority"
          :disabled="busy || !writable"
          @update:model-value="changePriority"
        />
        <p
          v-if="priorityInvalid"
          :id="`zram-priority-error-${device.name}`"
          class="text-destructive text-sm"
          aria-live="polite"
        >
          {{ t("priorityError") }}
        </p>
        <label
          class="flex min-h-12 cursor-pointer items-center justify-between gap-4 py-2 @max-[14rem]:flex-col @max-[14rem]:items-start"
        >
          <span class="min-w-0 text-sm">{{ t("automaticPriority") }}</span>
          <Switch
            v-model="automaticPriority"
            :aria-label="t('automaticPriority')"
            :disabled="busy || !writable"
          />
        </label>
      </div>
    </div>
    <p :id="`zram-input-help-${device.name}`" class="text-muted-foreground text-sm">
      {{ t("inputHelp", { max: range }) }}
    </p>
    <label
      class="flex min-h-12 cursor-pointer items-center justify-between gap-4 rounded-md py-2 @max-[14rem]:flex-col @max-[14rem]:items-start"
    >
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
