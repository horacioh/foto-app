import type { CoreBackend, CoreEvent, CoreSnapshot } from './types.ts'

export type Unsubscribe = () => void

/**
 * Typed wrapper over a {@link CoreBackend}. Serialises events, parses
 * snapshots and notifies subscribers after each dispatch so React can render
 * via `useSyncExternalStore`.
 */
export class CoreClient {
  #backend: CoreBackend
  #listeners = new Set<() => void>()
  #snapshot: CoreSnapshot

  constructor(backend: CoreBackend) {
    this.#backend = backend
    this.#snapshot = CoreClient.#parse(backend.stateJson())
  }

  get version(): string {
    return this.#backend.version()
  }

  dispatch(event: CoreEvent): void {
    this.#backend.dispatch(JSON.stringify(event))
    this.#snapshot = CoreClient.#parse(this.#backend.stateJson())
    for (const listener of this.#listeners) listener()
  }

  getSnapshot(): CoreSnapshot {
    return this.#snapshot
  }

  subscribe(listener: () => void): Unsubscribe {
    this.#listeners.add(listener)
    return () => this.#listeners.delete(listener)
  }

  static #parse(json: string): CoreSnapshot {
    const value: unknown = JSON.parse(json)
    if (
      typeof value !== 'object' ||
      value === null ||
      typeof (value as { version?: unknown }).version !== 'string' ||
      typeof (value as { pending_events?: unknown }).pending_events !== 'number'
    ) {
      throw new Error(`core returned an invalid snapshot: ${json}`)
    }
    return value as CoreSnapshot
  }
}
