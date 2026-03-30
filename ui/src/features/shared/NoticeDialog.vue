<script setup lang="ts">
const props = defineProps<{
  open: boolean
  title: string
  description?: string
  body?: string
  linkHref?: string
  linkLabel?: string
  linkText?: string
  closeLabel: string
}>()

const emit = defineEmits<{
  close: []
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
        </div>

        <div class="dialog-copy">
          <p v-if="props.body" class="body-copy">{{ props.body }}</p>
          <p v-if="props.linkText" class="mono-inline dialog-link-text">{{ props.linkText }}</p>
        </div>

        <div class="toolbar dialog-toolbar">
          <a
            v-if="props.linkHref && props.linkLabel"
            class="action-secondary"
            :href="props.linkHref"
            rel="noreferrer noopener"
            target="_blank"
          >
            {{ props.linkLabel }}
          </a>
          <button class="action-primary" type="button" @click="close">
            {{ props.closeLabel }}
          </button>
        </div>
      </section>
    </div>
  </Teleport>
</template>
