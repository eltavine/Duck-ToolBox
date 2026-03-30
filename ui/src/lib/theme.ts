import { computed, reactive } from "vue"

const STORAGE_KEY = "duck-toolbox/theme"
const DARK_MEDIA_QUERY = "(prefers-color-scheme: dark)"

export type Theme = "light" | "dark"

const state = reactive({
  theme: detectTheme(),
})

function readStoredTheme(): Theme | null {
  const saved = globalThis.localStorage?.getItem(STORAGE_KEY)
  return saved === "light" || saved === "dark" ? saved : null
}

function detectSystemTheme(): Theme {
  return globalThis.matchMedia?.(DARK_MEDIA_QUERY).matches ? "dark" : "light"
}

function detectTheme(): Theme {
  return readStoredTheme() ?? detectSystemTheme()
}

function applyTheme(theme: Theme) {
  globalThis.document?.documentElement?.setAttribute("data-theme", theme)
}

function syncWithSystemTheme() {
  const mediaQuery = globalThis.matchMedia?.(DARK_MEDIA_QUERY)
  if (!mediaQuery) {
    return
  }

  const handleChange = (event: MediaQueryListEvent) => {
    if (readStoredTheme()) {
      return
    }

    state.theme = event.matches ? "dark" : "light"
    applyTheme(state.theme)
  }

  if (typeof mediaQuery.addEventListener === "function") {
    mediaQuery.addEventListener("change", handleChange)
    return
  }

  ;(
    mediaQuery as MediaQueryList & {
      addListener?: (listener: (event: MediaQueryListEvent) => void) => void
    }
  ).addListener?.(handleChange)
}

syncWithSystemTheme()

export function initializeTheme() {
  applyTheme(state.theme)
}

export function useTheme() {
  const theme = computed({
    get: () => state.theme,
    set: (value: Theme) => {
      state.theme = value
      globalThis.localStorage?.setItem(STORAGE_KEY, value)
      applyTheme(value)
    },
  })

  return {
    theme,
  }
}
