<script setup lang="ts">
import { nextTick, ref } from "vue"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { useFakerI18n } from "../i18n"
import { profileSnippet } from "../profile"
import PresetPicker from "../presets/PresetPicker.vue"
import { fieldsFromPreset, type DevicePreset } from "../presets/catalog"

const emit = defineEmits<{ append: [content: string] }>()
const { t } = useFakerI18n()
const name = ref("")
const packages = ref("")
const fields = ref(fieldsFromPreset())
const primaryFields = [
  "manufacturer",
  "brand",
  "model",
  "device",
  "product",
  "fingerprint",
] as const
const extraFields = ["marketname", "name", "build_id"] as const
const extraOpen = ref(false)
const loaded = ref("")
const fieldRegion = ref<HTMLElement>()
const error = ref("")
async function loadPreset(preset: DevicePreset) {
  fields.value = fieldsFromPreset(preset)
  extraOpen.value = extraFields.some((key) => fields.value[key])
  loaded.value = preset.label
  error.value = ""
  await nextTick()
  fieldRegion.value?.focus({ preventScroll: true })
  fieldRegion.value?.scrollIntoView({ block: "nearest", behavior: "instant" })
}
function add() {
  try {
    emit("append", profileSnippet(name.value, packages.value, fields.value))
    error.value = ""
  } catch {
    error.value = t("profileRequired")
  }
}
</script>
<template>
  <details class="rounded-xl border p-[12px]">
    <summary
      class="flex min-h-12 cursor-pointer items-center rounded-lg text-sm font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      {{ t("addProfile") }}
    </summary>
    <div class="mt-4 flex flex-col gap-4">
      <label class="text-sm"
        >{{ t("templateName") }}<Input v-model="name" class="mt-2" dir="auto"
      /></label>
      <label class="text-sm"
        >{{ t("packages")
        }}<Textarea
          v-model="packages"
          class="mt-2"
          dir="ltr"
          spellcheck="false"
          autocapitalize="off"
      /></label>
      <p class="text-muted-foreground text-xs leading-5">{{ t("packageHelp") }}</p>
      <PresetPicker @select="loadPreset" />
      <p v-if="loaded" role="status" class="text-primary text-sm leading-6">
        {{ t("presets.loaded", { device: `\u2068${loaded}\u2069` }) }}
      </p>
      <div
        ref="fieldRegion"
        tabindex="-1"
        class="grid min-w-0 gap-4 rounded-xl outline-none focus-visible:ring-2 focus-visible:ring-ring sm:grid-cols-2"
        :aria-label="t('presets.reviewFields')"
        role="group"
      >
        <label v-for="key in primaryFields" :key="key" class="min-w-0 text-sm">
          {{ t(`fields.${key}`)
          }}<Input
            v-model="fields[key]"
            class="mt-2"
            :dir="key === 'device' || key === 'product' || key === 'fingerprint' ? 'ltr' : 'auto'"
            spellcheck="false"
            autocapitalize="off"
          />
        </label>
      </div>
      <details
        :open="extraOpen"
        class="min-w-0"
        @toggle="extraOpen = ($event.target as HTMLDetailsElement).open"
      >
        <summary
          class="flex min-h-12 cursor-pointer items-center rounded-lg text-sm font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {{ t("presets.moreFields") }}
        </summary>
        <div class="mt-3 grid min-w-0 gap-4 sm:grid-cols-2">
          <label v-for="key in extraFields" :key="key" class="min-w-0 text-sm"
            >{{ t(`fields.${key}`)
            }}<Input
              v-model="fields[key]"
              class="mt-2"
              :dir="key === 'marketname' ? 'auto' : 'ltr'"
              spellcheck="false"
              autocapitalize="off"
          /></label>
        </div>
      </details>
      <p v-if="error" role="alert" class="text-destructive text-sm">{{ error }}</p>
      <Button variant="outline" @click="add">{{ t("append") }}</Button>
    </div>
  </details>
</template>
