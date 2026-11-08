import { useQuery } from "@tanstack/vue-query"
import { bridge, type InstalledPackage } from "./bridge"

type AppRow = Pick<InstalledPackage, "packageName" | "label" | "userId">

function packageRows(apps: AppRow[]): AppRow[] {
  const rows = new Map<string, AppRow>()
  for (const { packageName, label } of apps) {
    if (!rows.has(packageName)) rows.set(packageName, { packageName, label })
  }
  return [...rows.values()]
}

/** Installed app instances and labels are independent of feature targeting rules. */
export function useInstalledApps(scope: "package" | "user" = "package") {
  return useQuery({
    queryKey: ["core", "installed-apps"],
    select: scope === "package" ? packageRows : undefined,
    queryFn: async ({ signal }) => {
      const host = bridge()
      const names = [...new Set(await host.listPackages("all"))]
      signal.throwIfAborted()
      const metadata = await host.packagesInfo(names, signal)
      const instances = new Map<string, Map<number | undefined, string>>()
      for (const { packageName, userId, label } of metadata) {
        const users = instances.get(packageName) ?? new Map<number | undefined, string>()
        users.set(userId, label || packageName)
        instances.set(packageName, users)
      }
      return names
        .flatMap((packageName): AppRow[] => {
          const users = instances.get(packageName)
          const known = [...(users ?? [])].flatMap(([userId, label]) =>
            userId === undefined ? [] : [{ packageName, label, userId }],
          )
          return known.length
            ? known
            : [{ packageName, label: users?.get(undefined) ?? packageName }]
        })
        .sort(
          (a, b) =>
            a.label.localeCompare(b.label) ||
            a.packageName.localeCompare(b.packageName) ||
            (a.userId ?? -1) - (b.userId ?? -1),
        )
    },
  })
}
