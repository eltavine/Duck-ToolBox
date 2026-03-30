<script setup lang="ts">
import { Check, X } from "lucide-vue-next"

interface ChoiceDialogOption {
  value: string | number
  label: string
  description: string
}

const props = defineProps<{
  open: boolean
  title: string
  description?: string
  options: ChoiceDialogOption[]
  selected: string | number | null
}>()

const emit = defineEmits<{
  close: []
  select: [value: string | number]
}>()

function close() {
  emit("close")
}

function onBackdropClick(event: MouseEvent) {
  if (event.target === event.currentTarget) {
    close()
  }
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="props.open"
      class="dialog-backdrop"
      @click="onBackdropClick"
    >
      <section
        class="dialog-card"
        role="dialog"
        aria-modal="true"
        :aria-label="props.title"
        @click.stop
      >
        <div class="panel-heading compact">
          <div>
            <h3 class="panel-title dialog-title">{{ props.title }}</h3>
            <p v-if="props.description" class="body-copy mt-2">{{ props.description }}</p>
          </div>
          <button class="icon-button" type="button" @click="close">
            <X class="size-4" />
          </button>
        </div>

        <div class="stack-list dialog-options">
          <button
            v-for="option in props.options"
            :key="String(option.value)"
            class="dialog-option"
            :class="{ 'is-active': option.value === props.selected }"
            type="button"
            @click="emit('select', option.value)"
          >
            <div>
              <strong class="dialog-option-title">{{ option.label }}</strong>
              <p class="body-copy mt-2">{{ option.description }}</p>
            </div>
            <Check v-if="option.value === props.selected" class="size-4" />
          </button>
        </div>
      </section>
    </div>
  </Teleport>
</template>
