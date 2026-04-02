<script setup lang="ts">
import { ref } from "vue"
import { ArrowUpRight, FolderRoot, Globe, Info, ShieldUser, X } from "lucide-vue-next"

import { useI18n } from "@/i18n"

const props = defineProps<{
  projectAddress: string
  repositoryUrl: string
  authors: ReadonlyArray<{
    name: string
    href: string
    label: string
  }>
  version: string
}>()

const { t } = useI18n()
const authorsDialogOpen = ref(false)

function openAuthorsDialog() {
  authorsDialogOpen.value = true
}

function closeAuthorsDialog() {
  authorsDialogOpen.value = false
}

function onAuthorsBackdropClick(event: MouseEvent) {
  if (event.target === event.currentTarget) {
    closeAuthorsDialog()
  }
}
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

      <button class="summary-tile about-author-card" type="button" @click="openAuthorsDialog">
        <div class="panel-heading compact">
          <span class="summary-label">{{ t("about.authors") }}</span>
          <ShieldUser class="icon-muted" />
        </div>
        <p class="mono-inline mt-2">{{ t("about.authorsHint") }}</p>
        <div class="choice-action mt-3">
          <span>{{ t("about.authorsCount", { count: props.authors.length }) }}</span>
          <ArrowUpRight class="size-4" />
        </div>
      </button>

      <article class="summary-tile">
        <div class="panel-heading compact">
          <span class="summary-label">{{ t("about.version") }}</span>
          <Info class="icon-muted" />
        </div>
        <p class="mono-inline mt-2">{{ version }}</p>
      </article>
    </div>
  </section>

  <Teleport to="body">
    <div
      v-if="authorsDialogOpen"
      class="dialog-backdrop"
      @click="onAuthorsBackdropClick"
    >
      <section
        class="dialog-card"
        role="dialog"
        aria-modal="true"
        :aria-label="t('about.authorsDialogTitle')"
        @click.stop
      >
        <div class="panel-heading compact">
          <div>
            <h3 class="panel-title dialog-title">{{ t("about.authorsDialogTitle") }}</h3>
            <p class="body-copy mt-2">{{ t("about.authorsDialogDescription") }}</p>
          </div>
          <button class="icon-button" type="button" @click="closeAuthorsDialog">
            <X class="size-4" />
          </button>
        </div>

        <div class="about-author-list">
          <a
            v-for="author in props.authors"
            :key="author.href"
            class="dialog-option about-author-link"
            :href="author.href"
            rel="noreferrer noopener"
            target="_blank"
          >
            <div>
              <strong class="dialog-option-title">{{ author.name }}</strong>
              <p class="mono-inline mt-2 break-all">{{ author.label }}</p>
            </div>
            <ArrowUpRight class="size-4 icon-muted" />
          </a>
        </div>

        <div class="toolbar dialog-toolbar">
          <button class="action-primary" type="button" @click="closeAuthorsDialog">
            {{ t("dialog.close") }}
          </button>
        </div>
      </section>
    </div>
  </Teleport>
</template>
