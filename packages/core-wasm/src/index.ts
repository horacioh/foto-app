/// <reference path="../types/pkg.d.ts" />
import type { CoreBackend } from '@photos/core'
import * as wasm from '#pkg'

export interface WasmBackendOptions {
  /**
   * Where to fetch `photos_core_wasm_bg.wasm` from. Bundlers that don't emit
   * `.wasm` assets next to the JS chunk (Metro) must serve it as a static file
   * and pass its URL here. Defaults to `import.meta.url`-relative resolution.
   */
  wasmUrl?: string | URL
}

/**
 * Instantiates the wasm-bindgen build of `photos-core` and adapts it to
 * {@link CoreBackend}. Only the small JS glue is bundled; the `.wasm` binary is
 * fetched when the web app boots the core.
 */
export async function createWasmBackend(options: WasmBackendOptions = {}): Promise<CoreBackend> {
  await wasm.default(
    options.wasmUrl === undefined ? undefined : { module_or_path: options.wasmUrl },
  )
  const core = new wasm.Core()
  return {
    version: () => wasm.version(),
    dispatch: (eventJson) => core.dispatch(eventJson),
    stateJson: () => core.state_json(),
  }
}
