import type { CoreBackend } from '@photos/core'
import { createMemoryBackend } from '@photos/core/memory'
import { createWasmBackend } from '@photos/core-wasm'

/**
 * Web: wasm-bindgen-backed core. The `.wasm` is served from `public/core/`
 * (copied there by `bun run wasm:build`). `EXPO_PUBLIC_CORE_BACKEND=memory`
 * skips the WASM build.
 */
export async function loadCore(): Promise<CoreBackend> {
  if (process.env.EXPO_PUBLIC_CORE_BACKEND === 'memory') return createMemoryBackend()
  return createWasmBackend({ wasmUrl: '/core/photos_core_wasm_bg.wasm' })
}
