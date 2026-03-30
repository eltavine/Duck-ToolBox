<script setup lang="ts">
import { Logs } from "lucide-vue-next"

import { useI18n } from "@/i18n"
import type { CommandHistoryEntry } from "@/lib/types"

defineProps<{
  history: CommandHistoryEntry[]
  lastError: string
}>()

const { t } = useI18n()
</script>

<template>
  <aside class="panel log-panel">
    <div class="panel-heading">
      <div>
        <div class="section-kicker">{{ t("shell.commandLogKicker") }}</div>
        <h2 class="panel-title">{{ t("shell.recentRuns") }}</h2>
      </div>
      <Logs class="icon-muted" />
    </div>

    <p v-if="lastError" class="muted mb-4">{{ lastError }}</p>

    <div v-if="history.length" class="stack-list">
      <article
        v-for="entry in history"
        :key="`${entry.at}-${entry.command}`"
        class="history-row"
      >
        <div>
          <div class="mono-inline text-sm">{{ entry.command }}</div>
          <p class="muted mt-2">{{ entry.message }}</p>
        </div>
        <div class="history-meta">
          <span class="mono-chip" :class="{ 'chip-error': !entry.ok }">
            {{ entry.ok ? "OK" : "ERR" }}
          </span>
          <span class="muted text-xs">{{ entry.at }}</span>
        </div>
      </article>
    </div>

    <p v-else class="empty-copy">
      {{ t("shell.noCommands") }}
    </p>
  </aside>
</template>
