<script setup lang="ts">
import { ref } from "vue"
import { useQuery, useQueryClient } from "@tanstack/vue-query"
import { useI18n } from "vue-i18n"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Field, FieldLabel } from "@/components/ui/field"
import QueryState from "@/core/components/QueryState.vue"
import ConfirmDialog from "@/core/components/ConfirmDialog.vue"
import { notifyError, notifySuccess } from "@/core/notify"
import { libraryApi } from "../library-api"
import { useIntegrityI18n } from "../i18n"
defineProps<{ content: string; preparing: boolean }>()
const emit = defineEmits<{ preview: [id: string] }>()
const { t } = useIntegrityI18n()
const global = useI18n()
const client = useQueryClient()
const profiles = useQuery({ queryKey: ["play-integrity.profiles"], queryFn: libraryApi.profiles })
const name = ref(""),
  busy = ref(false),
  removal = ref<string | null>(null),
  confirm = ref(false)
async function work(action: () => Promise<unknown>) {
  if (busy.value) return
  busy.value = true
  try {
    await action()
  } catch (error) {
    notifyError(error)
  } finally {
    busy.value = false
  }
}
async function save(content: string) {
  await work(async () => {
    await libraryApi.saveProfile(name.value, content)
    await client.invalidateQueries({ queryKey: ["play-integrity.profiles"] })
    name.value = ""
    notifySuccess(t("profiles.saved"))
  })
}
async function remove() {
  const id = removal.value
  if (!id) return
  await work(async () => {
    await libraryApi.deleteProfile(id)
    await client.invalidateQueries({ queryKey: ["play-integrity.profiles"] })
  })
}
function askRemove(id: string) {
  removal.value = id
  confirm.value = true
}
</script>
<template>
  <div class="flex flex-col gap-4">
    <p class="text-muted-foreground text-sm">{{ t("profiles.help") }}</p>
    <Field>
      <FieldLabel for="pif-profile-name">{{ t("profiles.name") }}</FieldLabel>
      <div class="flex flex-wrap gap-2">
        <Input id="pif-profile-name" v-model="name" maxlength="64" class="min-w-40 flex-1" />
        <Button
          variant="secondary"
          :disabled="
            busy ||
            preparing ||
            profiles.isPending.value ||
            !!profiles.error.value ||
            !name.trim() ||
            !content.trim() ||
            (profiles.data.value?.profiles.length ?? 0) +
              (profiles.data.value?.invalid_profiles.length ?? 0) >=
              12
          "
          @click="save(content)"
          >{{ t("profiles.save") }}</Button
        >
      </div>
      <div
        v-for="profile in profiles.data.value?.invalid_profiles"
        :key="profile.id"
        class="rounded-xl border p-3 text-sm"
      >
        <p>{{ t("profiles.invalid") }}</p>
        <p class="text-muted-foreground mt-1 font-mono text-xs break-all">{{ profile.id }}</p>
        <p class="text-muted-foreground mt-2 break-words">{{ profile.error }}</p>
        <Button
          variant="ghost"
          :disabled="busy || preparing"
          :aria-label="`${global.t('core.actions.remove')}: ${profile.id}`"
          @click="askRemove(profile.id)"
          >{{ global.t("core.actions.remove") }}</Button
        >
      </div>
    </Field>
    <QueryState
      :loading="profiles.isPending.value"
      :error="profiles.error.value"
      @retry="profiles.refetch()"
    >
      <p
        v-if="
          profiles.data.value &&
          !profiles.data.value.profiles.length &&
          !profiles.data.value.invalid_profiles.length
        "
        class="text-muted-foreground text-sm"
      >
        {{ t("profiles.empty") }}
      </p>
      <div
        v-for="profile in profiles.data.value?.profiles"
        :key="profile.id"
        class="flex flex-wrap items-center gap-2 rounded-xl border p-3"
      >
        <div class="min-w-0 flex-1">
          <p class="break-words font-medium">{{ profile.name }}</p>
          <p class="text-muted-foreground text-xs">
            {{ new Date(profile.created * 1000).toLocaleString(global.locale.value) }}
          </p>
        </div>
        <Button
          variant="secondary"
          :disabled="busy || preparing"
          :aria-label="`${t('preview')}: ${profile.name}`"
          @click="emit('preview', profile.id)"
          >{{ t("preview") }}</Button
        >
        <Button
          variant="ghost"
          :disabled="busy || preparing"
          :aria-label="`${global.t('core.actions.remove')}: ${profile.name}`"
          @click="askRemove(profile.id)"
          >{{ global.t("core.actions.remove") }}</Button
        >
      </div>
    </QueryState>
    <ConfirmDialog
      v-model:open="confirm"
      :title="t('profiles.remove')"
      :description="t('profiles.removeHelp')"
      :confirm-label="global.t('core.actions.remove')"
      destructive
      @confirm="remove"
    />
  </div>
</template>
