<script setup lang="ts">
import { useI18n } from "vue-i18n"

import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"

defineProps<{
  title: string
  description?: string
  confirmLabel?: string
  destructive?: boolean
}>()

const open = defineModel<boolean>("open", { required: true })
const emit = defineEmits<{ confirm: []; closeAutoFocus: [event: Event] }>()
const { t } = useI18n()

// Confirm before closing: callers read state that closing the dialog resets.
function accept() {
  emit("confirm")
  open.value = false
}
</script>

<template>
  <AlertDialog v-model:open="open">
    <AlertDialogContent @close-auto-focus="emit('closeAutoFocus', $event)">
      <AlertDialogHeader>
        <AlertDialogTitle>{{ title }}</AlertDialogTitle>
        <AlertDialogDescription
          v-if="description || $slots.description"
          class="whitespace-pre-line"
        >
          <slot name="description">{{ description }}</slot>
        </AlertDialogDescription>
        <slot />
      </AlertDialogHeader>
      <AlertDialogFooter>
        <AlertDialogCancel>{{ t("core.actions.cancel") }}</AlertDialogCancel>
        <Button :variant="destructive ? 'destructive' : 'default'" @click="accept">
          {{ confirmLabel ?? t("core.actions.confirm") }}
        </Button>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
</template>
