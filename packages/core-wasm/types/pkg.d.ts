/**
 * Fallback typings for the wasm-pack output in `../pkg` (gitignored, built by
 * `bun run --filter @photos/core-wasm build`). Once built, the generated
 * `.d.ts` takes precedence. Keep in sync with `crates/core-wasm/src/lib.rs`.
 */
declare module '#pkg' {
  export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module
  export default function init(
    module_or_path?:
      | { module_or_path: InitInput | Promise<InitInput> }
      | InitInput
      | Promise<InitInput>,
  ): Promise<unknown>
  export function version(): string
  export class Core {
    constructor()
    dispatch(event_json: string): void
    state_json(): string
    free(): void
  }
}
