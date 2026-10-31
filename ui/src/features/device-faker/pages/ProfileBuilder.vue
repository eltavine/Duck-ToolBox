<script setup lang="ts">
import { ref } from "vue"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { useFakerI18n } from "../i18n"
import { profileSnippet } from "../profile"

const emit = defineEmits<{ append: [content: string] }>()
const { t } = useFakerI18n()
const name = ref("")
const packages = ref("")
const fields = ref({
  manufacturer: "",
  brand: "",
  model: "",
  device: "",
  product: "",
  fingerprint: "",
})
const error = ref("")
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
      <div class="grid gap-4 sm:grid-cols-2">
        <label v-for="(_, key) in fields" :key="key" class="text-sm">
          {{ t(`fields.${key}`)
          }}<Input
            v-model="fields[key]"
            class="mt-2"
            dir="ltr"
            spellcheck="false"
            autocapitalize="off"
          />
        </label>
      </div>
      <p v-if="error" role="alert" class="text-destructive text-sm">{{ error }}</p>
      <Button variant="outline" @click="add">{{ t("append") }}</Button>
    </div>
  </details>
</template>
