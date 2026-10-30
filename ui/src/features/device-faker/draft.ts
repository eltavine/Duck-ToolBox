export interface Draft {
  content: string
  revision: string
}
export const MAX_CONFIG_BYTES = 48 * 1024
export const MAX_DRAFT_CHARACTERS = 48 * 1024
export const MAX_DRAFT_BYTES = 256 * 1024
export function readDraft(raw: string): Draft | null {
  if (raw.length > MAX_DRAFT_BYTES * 6 + 512) return null
  try {
    const value: unknown = JSON.parse(raw)
    if (!value || typeof value !== "object") return null
    const draft = value as Partial<Draft>
    return typeof draft.content === "string" &&
      typeof draft.revision === "string" &&
      draft.revision.length <= 128 &&
      new TextEncoder().encode(draft.content).length <= MAX_DRAFT_BYTES
      ? { content: draft.content, revision: draft.revision }
      : null
  } catch {
    return null
  }
}
