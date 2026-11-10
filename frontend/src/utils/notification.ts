import { createNotification } from '@codex-proxy/ui'
import { zhHans } from '@codex-proxy/ui/locale'

export const notification = createNotification({ locale: zhHans })

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}
