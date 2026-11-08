const packageOf = (key: string) => key.split("@")[0]!

/** An unscoped selection must not broaden an existing user-scoped target. */
export function existingTargets(keys: string): (key: string) => boolean {
  const targets = new Set(keys.split(/[\s,]+/).filter(Boolean))
  const packages = new Set([...targets].map(packageOf))
  return (key) => {
    const name = packageOf(key)
    return key === name ? packages.has(name) : targets.has(key) || targets.has(name)
  }
}

export function appendPackageKeys(keys: string, packages: readonly string[]): string {
  const existing = existingTargets(keys)
  const additions = [...new Set(packages)].filter((key) => !existing(key))
  const scoped = new Set(additions.filter((key) => key.includes("@")).map(packageOf))
  const added = additions.filter((key) => !scoped.has(key))
  if (!added.length) return keys
  return keys + (keys && !keys.endsWith("\n") ? "\n" : "") + added.join("\n")
}
