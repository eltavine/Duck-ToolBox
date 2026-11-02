/** User-scoped keys already target the app and must not gain a broader fallback implicitly. */
export function existingPackages(keys: string): Set<string> {
  return new Set(
    keys
      .split(/[\s,]+/)
      .filter(Boolean)
      .map((key) => key.split("@")[0]!),
  )
}

export function appendPackageKeys(keys: string, packages: readonly string[]): string {
  const existing = existingPackages(keys)
  const added = [...new Set(packages)].filter((name) => !existing.has(name))
  if (!added.length) return keys
  return keys + (keys && !keys.endsWith("\n") ? "\n" : "") + added.join("\n")
}
