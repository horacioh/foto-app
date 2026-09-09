# photos

Open source, end-to-end encrypted photo library with a sharing model that
actually works: any album is shareable, smart albums are shareable, the album
owner pays for storage, and shared photos show up in every member's library
without merging libraries. iOS, Android and web from one codebase.
Self-host it, or use the hosted version.

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first. It explains the
requirements, the encryption model, the data model, the state machine / actor
design, storage nodes (including machines behind Tailscale), hosted tiers and
the delivery phases.

## Layout

| Path | What |
|------|------|
| `apps/app` | Expo app (iOS, Android, web). React Native + StyleX via `react-strict-dom`. |
| `packages/core` | TypeScript facade over the Rust core (`dispatch` / `subscribe`). |
| `packages/core-wasm` | Web build of the Rust core (wasm-bindgen). |
| `packages/core-native` | Expo module wrapping the UniFFI bindings (iOS/Android). |
| `packages/ui` | Shared UI components. |
| `crates/core` | `photos-core`: state machines, actor runtime, crypto, sync. |
| `crates/protocol` | Wire types shared by clients, coordinator and storage node. |
| `crates/core-uniffi` | Mobile bindings (UniFFI). |
| `crates/core-wasm` | Web bindings (wasm-bindgen). |
| `crates/server` | Coordinator (axum + Postgres). |
| `crates/storage-node` | Encrypted blob store, single binary, pluggable backends. |

## Prerequisites

- [Bun](https://bun.sh) ≥ 1.4
- Rust stable (see `rust-toolchain.toml`; installs `wasm32-unknown-unknown`)
- `wasm-pack` for the web core: `cargo install wasm-pack`
- For iOS/Android: Xcode / Android Studio as per Expo docs

## Commands

```sh
bun install                 # JS deps
bun run lint                # biome
bun run typecheck           # tsc across workspaces
bun test                    # TS unit tests

bun run rust:test           # cargo test --workspace
bun run rust:lint           # clippy + fmt check
bun run storage-node        # run a storage node on :4100 (data in ./data)
bun run server              # run the coordinator on :4000

bun run wasm:build          # build the web core (required before running the app on web)
bun run app                 # expo start (press w for web, i / a for simulators)
```

Set `EXPO_PUBLIC_CORE_BACKEND=memory` to run the app against an in-memory JS
core (no wasm / native build needed). Native (iOS/Android) core bindings are
built with `packages/core-native/scripts/build-rust.sh` and are not yet wired
into CI.

CI (`.github/workflows/ci.yml`) runs fmt/clippy/tests for Rust, the wasm-pack
build, and lint/typecheck/tests plus a web export for TypeScript.

## License

Apache-2.0. See [LICENSE](LICENSE).
