<script setup lang="ts">
import type { DialogContentEmits, DialogContentProps } from "reka-ui"
import type { HTMLAttributes } from "vue"
import { useI18n } from "vue-i18n"
import { X } from "@lucide/vue"
import { reactiveOmit } from "@vueuse/core"
import {
  DialogClose,
  DialogContent,
  DialogPortal,
  useForwardPropsEmits,
} from "reka-ui"
import { buttonVariants } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import DialogOverlay from "./DialogOverlay.vue"

defineOptions({
  inheritAttrs: false,
})

const props = withDefaults(defineProps<DialogContentProps & { class?: HTMLAttributes["class"], showCloseButton?: boolean }>(), {
  showCloseButton: true,
})
const emits = defineEmits<DialogContentEmits>()
const { t } = useI18n()

const delegatedProps = reactiveOmit(props, "class")

const forwarded = useForwardPropsEmits(delegatedProps, emits)
</script>

<template>
  <DialogPortal>
    <DialogOverlay />
    <DialogContent
      data-slot="dialog-content"
      v-bind="{ ...$attrs, ...forwarded }"
      :class="
        cn(
          'bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 fixed top-[calc(var(--inset-top)+(100dvh-var(--inset-top)-var(--inset-bottom))/2)] left-[calc(var(--inset-left)+(100%-var(--inset-left)-var(--inset-right))/2)] z-50 grid max-h-[calc(100dvh-var(--inset-top)-var(--inset-bottom)-2rem)] min-h-0 w-[calc(100%-var(--inset-left)-var(--inset-right)-2rem)] min-w-0 max-w-140 translate-x-[-50%] translate-y-[-50%] grid-cols-[minmax(0,1fr)] gap-4 overflow-y-auto overscroll-contain rounded-2xl p-4 shadow-xl duration-200 sm:p-6',
          props.class,
        )"
    >
      <slot />

      <DialogClose
        v-if="showCloseButton"
        data-slot="dialog-close"
        :class="cn(buttonVariants({ variant: 'ghost', size: 'icon' }), 'text-muted-foreground absolute end-3 top-3')"
      >
        <X />
        <span class="sr-only">{{ t('core.actions.close') }}</span>
      </DialogClose>
    </DialogContent>
  </DialogPortal>
</template>
