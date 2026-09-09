import { CoreClient, type CoreSnapshot } from '@photos/core'
import { useEffect, useState, useSyncExternalStore } from 'react'
import { loadCore } from './core'

export type CoreStatus =
  | { kind: 'loading' }
  | { kind: 'error'; message: string }
  | { kind: 'ready'; client: CoreClient }

/** Boots the platform core once and exposes the client. */
export function useCore(): CoreStatus {
  const [status, setStatus] = useState<CoreStatus>({ kind: 'loading' })

  useEffect(() => {
    let cancelled = false
    loadCore()
      .then((backend) => {
        if (!cancelled) setStatus({ kind: 'ready', client: new CoreClient(backend) })
      })
      .catch((error: unknown) => {
        if (!cancelled) setStatus({ kind: 'error', message: String(error) })
      })
    return () => {
      cancelled = true
    }
  }, [])

  return status
}

const EMPTY: CoreSnapshot = { version: '', pending_events: 0 }

/** Subscribes a component to the core's snapshot. */
export function useCoreSnapshot(client: CoreClient | null): CoreSnapshot {
  return useSyncExternalStore(
    (onChange) => (client ? client.subscribe(onChange) : () => {}),
    () => (client ? client.getSnapshot() : EMPTY),
    () => EMPTY,
  )
}
