import { computed, onScopeDispose, shallowRef } from "vue"
import type { ConfirmRequest } from "@/core/confirm"

/** A pending decision belongs to this editor and is cancelled when it unmounts. */
export function useLanguageConfirmation() {
  type Pending = ConfirmRequest & { resolve: (accepted: boolean) => void }
  const pending = shallowRef<Pending | null>(null)
  function confirm(request: ConfirmRequest): Promise<boolean> {
    pending.value?.resolve(false)
    return new Promise((resolve) => {
      const item: Pending = {
        ...request,
        resolve: (accepted) => {
          if (pending.value === item) pending.value = null
          resolve(accepted)
        },
      }
      pending.value = item
    })
  }
  const open = computed({
    get: () => pending.value !== null,
    set: (value: boolean) => {
      if (!value) pending.value?.resolve(false)
    },
  })
  onScopeDispose(() => pending.value?.resolve(false))
  return { pending, open, confirm, accept: () => pending.value?.resolve(true) }
}
