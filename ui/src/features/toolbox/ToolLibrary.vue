<script setup lang="ts">
import { useI18n } from "@/i18n"

interface ToolDescriptor {
  id: string
  name: string
  category: string
  summary: string
  capabilities: string[]
}

defineProps<{
  activeTool: string
  tools: ToolDescriptor[]
}>()

defineEmits<{
  select: [toolId: string]
}>()

const { t } = useI18n()
</script>

<template>
  <section class="panel">
    <div class="panel-heading">
      <div>
        <div class="section-kicker">{{ t("shell.toolLibraryKicker") }}</div>
        <h2 class="panel-title">{{ t("shell.toolLibraryTitle") }}</h2>
      </div>
    </div>

    <p class="body-copy">{{ t("shell.toolLibraryBody") }}</p>

    <div class="tool-list">
      <button
        v-for="tool in tools"
        :key="tool.id"
        class="tool-card"
        :class="{ 'is-active': activeTool === tool.id }"
        type="button"
        @click="$emit('select', tool.id)"
      >
        <div class="flex items-start justify-between gap-3">
          <div>
            <div class="section-kicker">{{ tool.category }}</div>
            <h3 class="subheading mt-2">{{ tool.name }}</h3>
          </div>
          <span class="mono-chip">{{ t("tool.installed") }}</span>
        </div>

        <p class="muted mt-3">{{ tool.summary }}</p>

        <div class="chip-row mt-4">
          <span
            v-for="capability in tool.capabilities"
            :key="capability"
            class="tool-pill"
          >
            {{ capability }}
          </span>
        </div>
      </button>
    </div>
  </section>
</template>
