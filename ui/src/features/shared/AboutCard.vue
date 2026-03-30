<script setup lang="ts">
import { ArrowUpRight, FolderRoot, Globe, Info, ShieldUser } from "lucide-vue-next"

import { useI18n } from "@/i18n"

defineProps<{
  projectAddress: string
  repositoryUrl: string
  authors: string
  version: string
}>()

const { t } = useI18n()
</script>

<template>
  <section class="panel">
    <div class="panel-heading">
      <div>
        <div class="section-kicker">{{ t("about.kicker") }}</div>
        <h2 class="panel-title">{{ t("about.title") }}</h2>
      </div>
      <Info class="icon-muted" />
    </div>

    <div class="summary-grid wide about-grid">
      <article class="summary-tile">
        <div class="panel-heading compact">
          <span class="summary-label">{{ t("about.projectAddress") }}</span>
          <FolderRoot class="icon-muted" />
        </div>
        <p class="mono-inline mt-2 break-all">{{ projectAddress }}</p>
      </article>

      <article class="summary-tile">
        <div class="panel-heading compact">
          <span class="summary-label">{{ t("about.repository") }}</span>
          <Globe class="icon-muted" />
        </div>
        <p class="mono-inline mt-2 break-all">{{ repositoryUrl }}</p>
        <div class="about-link-row">
          <a
            class="action-secondary about-link"
            :href="repositoryUrl"
            rel="noreferrer noopener"
            target="_blank"
          >
            <ArrowUpRight class="size-4" />
            {{ t("actions.openRepository") }}
          </a>
        </div>
      </article>

      <article class="summary-tile">
        <div class="panel-heading compact">
          <span class="summary-label">{{ t("about.authors") }}</span>
          <ShieldUser class="icon-muted" />
        </div>
        <p class="mono-inline mt-2">{{ authors }}</p>
      </article>

      <article class="summary-tile">
        <div class="panel-heading compact">
          <span class="summary-label">{{ t("about.version") }}</span>
          <Info class="icon-muted" />
        </div>
        <p class="mono-inline mt-2">{{ version }}</p>
      </article>
    </div>
  </section>
</template>
