<script setup lang="ts">
import { Copy, X } from "lucide-vue-next"

const props = defineProps<{
  open: boolean
  title: string
  description?: string
  content: string
  copyLabel: string
  closeLabel: string
}>()

const emit = defineEmits<{
  close: []
  copy: []
}>()
</script>

<template>
  <Teleport to="body">
    <div
      v-if="props.open"
      class="dialog-backdrop"
      @click="emit('close')"
    >
      <section
        class="dialog-card dialog-card-wide"
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
          <button class="icon-button" type="button" @click="emit('close')">
            <X class="size-4" />
          </button>
        </div>

        <pre class="dialog-code">{{ props.content }}</pre>

        <div class="toolbar dialog-toolbar">
          <button class="action-secondary" type="button" @click="emit('copy')">
            <Copy class="size-4" />
            {{ props.copyLabel }}
          </button>
          <button class="action-primary" type="button" @click="emit('close')">
            {{ props.closeLabel }}
          </button>
        </div>
      </section>
    </div>
  </Teleport>
</template>
