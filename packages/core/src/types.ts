/**
 * Wire types shared with the Rust core. Every event/state crosses the FFI as
 * JSON, so these must stay in sync with `crates/core` serde shapes
 * (ARCHITECTURE.md §7, "Actor ↔ UI bridge").
 */

export type AssetId = string
export type AlbumId = string
export type DeviceId = string

/** Events the UI can send into the core. */
export type CoreEvent =
  | { type: 'app/started'; device_id: DeviceId }
  | { type: 'app/backgrounded' }
  | { type: 'app/foregrounded' }
  | { type: 'upload/discovered'; asset_id: AssetId; size: number; local_uri: string }
  | { type: 'upload/cancel'; asset_id: AssetId }
  | { type: 'album/create'; name: string }
  | { type: 'album/share'; album_id: AlbumId; account: string; role: 'contributor' | 'viewer' }
  | { type: 'sync/now' }

/** Snapshot the UI renders. Grows as core machines are wired in (phase 1). */
export interface CoreSnapshot {
  version: string
  pending_events: number
}

/**
 * Minimal platform binding surface. Implemented by `@photos/core-native`
 * (UniFFI) and `@photos/core-wasm` (wasm-bindgen), and by the in-memory
 * mock in `./memory.ts`. Everything is JSON strings on purpose: one type layer
 * on the TS side, one serde layer on the Rust side.
 */
export interface CoreBackend {
  version(): string
  dispatch(eventJson: string): void
  stateJson(): string
}
