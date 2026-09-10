import type { CoreBackend } from './types.ts'

/**
 * Pure-TS stand-in for the Rust core. Mirrors the current binding skeleton
 * (validates JSON, counts events) so UI and tests can run without a native or
 * WASM build. Never ship this to users.
 */
export function createMemoryBackend(version = '0.0.0-memory'): CoreBackend {
  const events: string[] = []
  return {
    version: () => version,
    dispatch(eventJson) {
      JSON.parse(eventJson)
      events.push(eventJson)
    },
    stateJson: () => JSON.stringify({ version, pending_events: events.length }),
  }
}
