export const MAX_PARAMETER_BYTES = 4096

/** Matches the backend's UTF-8 byte budget and Rust char::is_control policy. */
export function parameterIssue(value: string): "parametersTooLong" | "parametersControl" | null {
  if (new TextEncoder().encode(value).length > MAX_PARAMETER_BYTES) return "parametersTooLong"
  for (const character of value) {
    const point = character.codePointAt(0)!
    if (point <= 0x1f || (point >= 0x7f && point <= 0x9f)) return "parametersControl"
  }
  return null
}
