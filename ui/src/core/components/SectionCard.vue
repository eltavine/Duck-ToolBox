<script setup lang="ts">
defineProps<{
  title?: string
  description?: string
  contentClass?: string
  /** The content brings its own surfaces, like a segmented list. */
  plain?: boolean
}>()
</script>

<template>
  <!-- Like a Material 3 settings group: the heading sits above the surface it names. -->
  <section class="flex min-w-0 flex-col gap-2">
    <header
      v-if="title || description || $slots.action"
      class="flex min-h-8 flex-wrap items-center gap-3 px-4"
    >
      <div class="min-w-0 flex-[1_1_12rem]">
        <h2 v-if="title" class="text-base font-semibold leading-6">{{ title }}</h2>
        <p v-if="description" class="text-muted-foreground text-sm break-words">
          {{ description }}
        </p>
      </div>
      <slot name="action" />
    </header>
    <div
      :class="[
        'min-w-0',
        plain ? 'flex flex-col' : 'bg-card text-card-foreground rounded-lg p-4',
        contentClass,
      ]"
    >
      <slot />
    </div>
  </section>
</template>
