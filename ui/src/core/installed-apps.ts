import { useQuery } from "@tanstack/vue-query"
import { bridge } from "./bridge"

/** Installed app names and labels are shared host data, independent of feature rules. */
export function useInstalledApps() {
  return useQuery({
    queryKey: ["core", "installed-apps"],
    queryFn: async ({ signal }) => {
      const host = bridge()
      const names = [...new Set(await host.listPackages("all"))]
      signal.throwIfAborted()
      const metadata = await host.packagesInfo(names, signal)
      const labels = new Map(metadata.map((app) => [app.packageName, app.label]))
      return names
        .map((packageName) => ({ packageName, label: labels.get(packageName) || packageName }))
        .sort(
          (a, b) => a.label.localeCompare(b.label) || a.packageName.localeCompare(b.packageName),
        )
    },
  })
}
