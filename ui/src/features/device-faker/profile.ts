/** Build an editable TOML candidate; publication always goes through engine validation. */
export function profileSnippet(
  name: string,
  packages: string,
  fields: Record<string, string>,
): string {
  const targets = packages.split(/[\s,]+/).filter(Boolean)
  if (!targets.length) throw new Error("package_required")
  const properties = Object.entries(fields)
    .filter(([, value]) => value.trim())
    .map(([key, value]) => `${key} = ${JSON.stringify(value.trim())}`)
  if (!properties.length) throw new Error("field_required")
  if (name.trim()) {
    return `\n[templates.${JSON.stringify(name.trim())}]\npackages = ${JSON.stringify(targets)}\n${properties.join("\n")}\n`
  }
  return targets
    .map((target) => `\n[[apps]]\npackage = ${JSON.stringify(target)}\n${properties.join("\n")}\n`)
    .join("")
}
