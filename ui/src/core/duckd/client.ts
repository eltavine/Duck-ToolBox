import { z } from "zod"

import { bridge, heredoc, shellQuote } from "@/core/bridge"

import { DuckError, ENVELOPE_API, parseEnvelope } from "./envelope"

export const MODULE_ID = import.meta.env.MODE === "lite" ? "duck-toolbox-lite" : "duck-toolbox"
const DEFAULT_MODULE_DIR = `/data/adb/modules/${MODULE_ID}`

/**
 * Our module directory. The WebUI may be opened from a keystore module whose `webroot`
 * links here (Tricky Addon style), but the backend always lives in our own module.
 */
export function moduleDir(): string {
  const host = bridge().hostModule()
  return host?.id === MODULE_ID && host.dir ? host.dir : DEFAULT_MODULE_DIR
}

export interface DuckdOptions<T> {
  /** Sent as JSON on stdin; `--stdin-json` is appended automatically. */
  input?: unknown
  /** Validates `data`. Unknown fields are dropped, so additive backend changes are safe. */
  schema?: z.ZodType<T>
}

export function commandLabel(args: readonly string[]): string {
  return args.filter((arg) => !arg.startsWith("-")).join(".")
}

/** Runs `duckd <args>` through the module wrapper and returns the validated `data`. */
export async function duckd<T = unknown>(
  args: readonly string[],
  options: DuckdOptions<T> = {},
): Promise<T> {
  const label = commandLabel(args)
  const host = bridge()
  if (!host.available) {
    throw new DuckError("host_unavailable", "no root WebUI host is available", label)
  }

  const argv = options.input === undefined ? [...args] : [...args, "--stdin-json"]
  const command = [`${moduleDir()}/bin/duckctl.sh`, ...argv].map(shellQuote).join(" ")
  const shell =
    options.input === undefined ? command : heredoc(command, JSON.stringify(options.input))

  const result = await host.exec(shell)
  const envelope = parseEnvelope(result.stdout, result.stderr, label, result.errno)

  if (envelope.api !== undefined && envelope.api !== ENVELOPE_API) {
    throw new DuckError(
      "api_mismatch",
      `backend envelope v${envelope.api} is not supported (expected v${ENVELOPE_API})`,
      envelope.command,
      { expected: ENVELOPE_API, received: envelope.api },
    )
  }
  if (!envelope.ok) {
    throw new DuckError(
      envelope.error?.code ?? "command_failed",
      envelope.error?.message ?? "command failed",
      envelope.command,
      envelope.error?.details,
    )
  }
  if (!options.schema) return envelope.data as T

  const parsed = options.schema.safeParse(envelope.data)
  if (!parsed.success) {
    throw new DuckError(
      "contract_mismatch",
      z.prettifyError(parsed.error),
      envelope.command,
      parsed.error.issues,
    )
  }
  return parsed.data
}
