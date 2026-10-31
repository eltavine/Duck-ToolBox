<script setup lang="ts">
import { ref, watch } from "vue"
import { useI18n } from "vue-i18n"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog"
import { Field, FieldLabel, FieldDescription } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { displayName } from "@/core/i18n"
import { useMiscI18n } from "../i18n"
import type { LanguageRule } from "./api"
import AppPicker from "./AppPicker.vue"

const props = defineProps<{ rule: LanguageRule | null }>()
const open = defineModel<boolean>("open", { required: true })
const emit = defineEmits<{ save: [rule: LanguageRule] }>()
const { t } = useMiscI18n()
const { t: coreT } = useI18n()
const packageName = ref("")
const userId = ref<string | number>(0)
const locale = ref("en-US")
const picking = ref(false)
const presets = [
  "en-US",
  "en-GB",
  "ja-JP",
  "ko-KR",
  "de-DE",
  "fr-FR",
  "es-ES",
  "zh-CN",
  "zh-TW",
  "ar",
]
watch(open, (value) => {
  if (!value) return
  packageName.value = props.rule?.package_name ?? ""
  userId.value = props.rule?.user_id ?? 0
  locale.value = props.rule?.locale ?? "en-US"
  picking.value = false
})
function submit() {
  emit("save", {
    package_name: packageName.value.trim(),
    user_id: Number(userId.value),
    locale: locale.value.trim(),
  })
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent @open-auto-focus.prevent>
      <DialogHeader>
        <DialogTitle>{{ t("language.rule") }}</DialogTitle>
        <DialogDescription>{{ t("language.ruleHelp") }}</DialogDescription>
      </DialogHeader>
      <form class="flex min-w-0 flex-col gap-5" @submit.prevent="submit">
        <Field>
          <FieldLabel for="language-package">{{ t("language.package") }}</FieldLabel>
          <Input
            id="language-package"
            v-model="packageName"
            required
            maxlength="255"
            pattern="[a-zA-Z][a-zA-Z0-9_]*(\.[a-zA-Z][a-zA-Z0-9_]*)+"
            dir="ltr"
            autocapitalize="off"
            spellcheck="false"
            placeholder="com.example.app"
          />
          <Button
            type="button"
            variant="outline"
            class="self-start"
            :aria-expanded="picking"
            @click="picking = !picking"
            >{{ t("language.pickApp") }}</Button
          >
          <AppPicker
            v-if="picking"
            @select="
              (name) => {
                packageName = name
                picking = false
              }
            "
          />
        </Field>
        <Field>
          <FieldLabel for="language-user">{{ t("language.user") }}</FieldLabel>
          <Input
            id="language-user"
            v-model="userId"
            type="number"
            min="0"
            max="21473"
            step="1"
            required
            dir="ltr"
          />
          <FieldDescription>{{ t("language.userHelp") }}</FieldDescription>
        </Field>
        <Field>
          <FieldLabel for="language-tag">{{ t("language.locale") }}</FieldLabel>
          <Select v-model="locale">
            <SelectTrigger class="w-full" :aria-label="t('language.presets')"
              ><SelectValue
            /></SelectTrigger>
            <SelectContent
              ><SelectItem v-for="tag in presets" :key="tag" :value="tag">{{
                displayName(tag)
              }}</SelectItem></SelectContent
            >
          </Select>
          <Input
            id="language-tag"
            v-model="locale"
            required
            maxlength="12"
            pattern="[a-zA-Z]{2,3}(-[a-zA-Z]{4})?(-([a-zA-Z]{2}|[0-9]{3}))?"
            dir="ltr"
            autocapitalize="off"
            spellcheck="false"
          />
          <FieldDescription>{{ t("language.localeHelp") }}</FieldDescription>
        </Field>
        <DialogFooter>
          <Button type="button" variant="outline" @click="open = false">{{
            coreT("core.actions.cancel")
          }}</Button>
          <Button type="submit">{{ coreT("core.actions.done") }}</Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>
