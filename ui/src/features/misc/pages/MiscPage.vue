<script setup lang="ts">
import { ChevronRight, Languages } from "@lucide/vue"
import { useRouter } from "vue-router"
import { useMiscI18n } from "../i18n"

const { t } = useMiscI18n()
const router = useRouter()
</script>

<template>
  <div class="mx-auto flex w-full max-w-3xl flex-col gap-6 p-4 pb-8">
    <button
      type="button"
      class="bg-card text-card-foreground hover:bg-muted focus-visible:ring-ring flex min-h-24 w-full items-center gap-4 rounded-lg p-4 text-start transition-colors focus-visible:ring-2 focus-visible:outline-none"
      @click="router.push('/misc/language')"
    >
      <span
        class="bg-primary/10 text-primary flex size-12 shrink-0 items-center justify-center rounded-2xl"
      >
        <Languages class="size-6" aria-hidden="true" />
      </span>
      <span class="flex min-w-0 flex-1 flex-col gap-1">
        <span class="font-semibold">{{ t("language.title") }}</span>
        <span class="text-muted-foreground text-sm leading-6">{{ t("language.description") }}</span>
      </span>
      <ChevronRight
        class="text-muted-foreground size-5 shrink-0 rtl:rotate-180"
        aria-hidden="true"
      />
    </button>
  </div>
</template>
