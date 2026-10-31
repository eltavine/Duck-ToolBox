<script setup lang="ts">
import type { Preview } from "../api"
import { useI18n } from "vue-i18n"
import { computed } from "vue"
import { useFakerI18n } from "../i18n"
const props = defineProps<{ summary: Preview["summary"] | null | undefined }>()
const { t, locale } = useFakerI18n()
const { n } = useI18n()
const templateNames = computed(() =>
  new Intl.ListFormat(locale.value, { style: "short", type: "conjunction" }).formatToParts(
    props.summary?.templates ?? [],
  ),
)
</script>
<template>
  <div class="flex min-w-0 flex-col gap-4">
    <div class="flex flex-wrap gap-2 text-sm">
      <span class="bg-secondary text-secondary-foreground rounded-full px-3 py-1.5">{{
        t("targetCount", { count: n(summary?.targets.length ?? 0) })
      }}</span>
      <span class="bg-muted rounded-full px-3 py-1.5"
        >{{ t("templates") }}: {{ n(summary?.templates.length ?? 0) }}</span
      >
    </div>
    <p
      v-if="summary?.templates.length"
      class="text-muted-foreground text-sm [overflow-wrap:anywhere]"
    >
      <template v-for="(part, index) in templateNames" :key="index">
        <bdi v-if="part.type === 'element'" dir="auto">{{ part.value }}</bdi
        ><span v-else>{{ part.value }}</span>
      </template>
    </p>
    <ul
      v-if="summary?.targets.length"
      class="flex max-h-48 flex-col gap-2 overflow-auto font-mono text-sm"
      dir="ltr"
      :aria-label="t('packages')"
    >
      <li v-for="target in summary.targets" :key="target" class="[overflow-wrap:anywhere]">
        {{ target }}
      </li>
    </ul>
    <ul
      v-if="summary?.warnings.length"
      class="bg-muted flex flex-col gap-3 rounded-2xl p-4 text-sm leading-6"
      :aria-label="t('warningsTitle')"
    >
      <li
        v-for="(warning, index) in summary.warnings"
        :key="`${warning.code}-${index}`"
        class="[overflow-wrap:anywhere]"
      >
        {{
          t(`warnings.${warning.code}`, {
            target: warning.target ? `\u2066${warning.target}\u2069` : undefined,
            count: warning.count === undefined ? undefined : n(warning.count),
          })
        }}
      </li>
    </ul>
  </div>
</template>
