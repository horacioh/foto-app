# Architecture

Working codename: **photos** (rename freely; nothing depends on the name).

An open source (Apache-2.0), end-to-end encrypted photo library that fixes the
sharing model described in
[How to Fix Apple Photos](https://seed.hyper.media/hm/z6MkgYR76i3w2Xc2fcvgFJWfYjsop6YwUonDhQx3AdLmQ4BQ/notes/how-to-fix-apple-photos):
any album is shareable, smart albums are shareable, one person pays for a shared
album's storage, and shared photos show up in every member's library without
merging libraries.

Runs on iOS, Android and the web from one codebase. Self-hostable end to end,
with a paid hosted option. Storage can live anywhere: our cloud, an S3 bucket,
or a box in your closet reachable over Tailscale.

---

## 1. Requirements

### From the source document

| # | Requirement | Design answer |
|---|-------------|---------------|
| R1 | No distinction between shared and normal albums. Any album is shareable, keeps full metadata, is searchable, can be nested in other albums. | An *album* is the only collection primitive. Sharing is a property of an album (its member list), not a different object. Metadata travels with the asset key, so members can search and index it locally. |
| R2 | Smart albums: rule-based membership (e.g. "all photos with this face"), also shareable. | Rules are encrypted and evaluated on the owner's devices; results materialize as regular album membership entries, so sharing is identical to R1. |
| R3 | Only the album owner pays for storage, even for photos other members upload. | Blobs uploaded into an album are placed on the owner's storage and counted against the owner's quota, via a scoped *upload grant*. |
| R4 | Shared photos appear in members' libraries and participate in "Optimize Storage". | Clients merge shared album feeds into the local library index; the storage evictor treats remote-available shared assets exactly like own assets. |
| R5 | Share "Recents" with a partner without merging libraries. | Recents is a built-in smart album (`captured_at > now-30d AND source = camera`); share it like any album. |
| R6 | Collect photos from many devices/people into one place while contributors keep access in their own libraries. | Contributor uploads go to the owner's storage (R3); the contributor keeps a normal album membership (R4). |
| R7 | Easy backup/export to another service. | Every client can stream-decrypt and export originals + metadata sidecars; storage nodes can mirror to any S3 target. |

### Additional requirements (project owner)

| # | Requirement | Design answer |
|---|-------------|---------------|
| A1 | Apache-2.0, fully open source. | Everything in this repo, including the billing code for the hosted tier. |
| A2 | Self-hostable and a paid hosted option. | Same binaries. Hosted = coordinator + storage nodes we run, with billing enabled via config. |
| A3 | iOS and Android (and web, decided during planning). | One Expo app targeting iOS, Android and web. |
| A4 | State machines + actor model for all logic, to make sync tractable. | All non-UI logic is a set of pure, typed state machines driven by an actor runtime in the Rust core. UI only renders state and sends events. |
| A5 | Storage is the expensive part; let people bring their own servers or machines behind Tailscale. | Storage node is a standalone single binary that registers with a coordinator. Tailscale-reachable nodes are first-class. |
| A6 | Frontend: Bun, TypeScript, React Native, StyleX, monorepo. Backend: Go or Rust, whichever is more performant and simple. | Rust. See §3 for why. |
| A7 | End-to-end encrypted ("if E2EE is the safest, go for it"). | Yes. The server never holds a key that can decrypt user content. |

### v1 scope (all decided in)

Auto-upload + timeline, album sharing (R1/R3/R4), smart albums (R2), video,
BYO storage node over Tailscale, optimize-storage, on-device faces, on-device
semantic search, importers (iCloud export, Google Takeout). Sequenced in §11.

### Non-goals (v1)

- Server-side ML of any kind (impossible under E2EE by design).
- Collaborative photo *editing*; edits are stored as non-destructive
  adjustment metadata, applied on the client.
- Federation between independent coordinators. One account lives on one
  coordinator. (Storage nodes are already federated.)
- Replacing iCloud Drive / generic file sync.

---

## 2. System overview

```
┌──────────────────────────────┐        ┌──────────────────────────────┐
│  Client (iOS / Android / Web)│        │  Coordinator (Rust, axum)    │
│  ┌────────────────────────┐  │  HTTPS │  - accounts, devices, keys   │
│  │ React Native + StyleX  │  │◀──────▶│  - album change feeds        │
│  └──────────┬─────────────┘  │  + WS  │  - storage node registry     │
│             │ events/state   │        │  - upload grants, quotas     │
│  ┌──────────▼─────────────┐  │        │  - billing (hosted only)     │
│  │ photos-core (Rust)     │  │        │  Postgres                    │
│  │ actors + state machines│  │        └──────────────┬───────────────┘
│  │ crypto, sync, index    │  │                       │ registration,
│  └──────────┬─────────────┘  │                       │ health, grants
│  UniFFI (native) / WASM (web)│                       ▼
└──────────────┬───────────────┘        ┌──────────────────────────────┐
               │ encrypted blobs        │  Storage node (Rust, single  │
               │ (direct, or via        │  binary)                     │
               │  tailnet / relay)      │  - PUT/GET encrypted blobs   │
               └───────────────────────▶│  - backends: disk, S3, R2    │
                                        │  - runs anywhere incl. over  │
                                        │    Tailscale                 │
                                        └──────────────────────────────┘
```

Three deployable things:

1. **Client app** (`apps/app`): Expo app for iOS, Android and web. Renders
   state, captures user events, hosts the native modules (camera roll,
   background transfer, ML runtimes). Embeds `photos-core`.
2. **Coordinator** (`crates/server`): the only stateful "brain" server. Holds
   accounts, public keys, encrypted key material, encrypted album change
   feeds, the storage node registry, quotas, and billing. Never sees plaintext
   content or metadata.
3. **Storage node** (`crates/storage-node`): dumb, content-addressed encrypted
   blob store with pluggable backends. Registers with a coordinator, accepts
   uploads authorized by coordinator-signed grants.

The hosted product is a coordinator plus a fleet of storage nodes we run.
Self-hosting is the same coordinator plus one or more nodes the user runs.
Users on the hosted coordinator can *also* register their own storage nodes
(cheaper tier, §10).

---

## 3. Language and stack decisions

### Rust for the core, server and storage node

Go would be simpler for a server-only backend. The deciding factor is E2EE +
A4: the security-critical code (key management, encryption, sync state
machines) must run **inside every client** as well as on the server. Writing
it once in Rust and compiling it three ways is safer than maintaining a
TypeScript copy and a Go copy in lockstep:

| Target | Mechanism |
|--------|-----------|
| iOS / Android | [UniFFI](https://mozilla.github.io/uniffi-rs/) → Swift/Kotlin bindings, wrapped in an Expo Module (`packages/core-native`). |
| Web | `wasm-bindgen` → `packages/core-wasm`. Crypto and state machines run in a Web Worker. |
| Server / storage node | Plain crate dependency. |

Performance-wise the hot paths are AEAD encryption of large files (Rust's
`chacha20poly1305` with SIMD is very fast), hashing, and thumbnail/transcode
work (delegated to platform codecs on mobile, `libvips`/`ffmpeg` on the
importer CLI). Rust is at or above Go on all of these and gives us the
`Send + Sync` guarantees that make the actor runtime safe.

Server framework: `axum` + `tokio` + `sqlx` (Postgres). Storage node:
`axum` with a `Backend` trait (`LocalDisk`, `S3`).

### TypeScript / React Native / StyleX

- **Bun** workspaces for the monorepo, Bun as the script runner and test
  runner for TS packages. Metro (Expo) still does bundling; Bun does not.
- **Expo** (not bare RN). Native modules via Expo Modules API and config
  plugins; EAS or local builds. Web target comes from the same app.
- **StyleX via `react-strict-dom`** so one component tree runs on native and
  web. Caveat, agreed during planning: `react-strict-dom` is still
  pre-1.0. If it bites, the fallback is RN `StyleSheet` on native and StyleX
  on web behind the same `packages/ui` component API.
- **XState** is *not* used for domain logic (that is in Rust). It may be
  used for purely visual flows (onboarding wizard, pickers) where a TS
  machine is more convenient than a Rust one.

---

## 4. Security and encryption model

Everything the server stores about content is ciphertext. The server does
learn: account identifiers, device counts, blob ids and sizes, upload
timestamps, and the album membership graph (who shares with whom). This is
the same leak profile as Ente and is documented as such in the threat model.

### Keys

```
passphrase ──Argon2id──▶ KEK (never leaves the device)
                            │
                            ▼ wraps
      ┌───────────────── Master Key (MK, random 256-bit) ─────────────────┐
      │                              │                                     │
      ▼ wraps                        ▼ wraps                               ▼ wraps
Identity keypair            Signing keypair                      Per-album keys (AK)
(X25519, for sealed boxes)  (Ed25519, signs change entries)              │
                                                                         ▼ wraps
                                                              Per-asset keys (K_asset)
                                                                         │
                                                                         ▼ encrypts
                                                     original, preview, thumb, metadata
```

- **Master key** is random; the passphrase only wraps it (so the passphrase
  can change without re-encrypting anything). A 24-word **recovery key**
  wraps the MK a second time. Both wrapped copies are stored on the
  coordinator.
- **Per-asset key** `K_asset` (random) encrypts each of the asset's blobs
  independently with XChaCha20-Poly1305 using the STREAM/secretstream chunked
  construction (chunk size 4 MiB) so uploads resume per chunk and
  downloads decrypt progressively (video scrubbing).
- **Album key** `AK` wraps `K_asset` for every asset in the album
  (`AlbumAsset.wrapped_key`). Adding an asset to an album = wrapping its key
  under `AK`. Removing = deleting the wrap. An asset in N albums has N wraps.
- **Sharing** an album = sealed-box (`crypto_box_seal`) `AK` to each member's
  X25519 identity key. Membership changes are signed by the owner.
- **Removing a member** rotates `AK` → `AK'`; the owner's device lazily
  re-wraps asset keys (a state machine, §7). Members who already downloaded
  content keep it; that is inherent to E2EE and is stated in the UI.
- **Multi-device**: a new device gets the MK by (a) entering the passphrase,
  or (b) approving from an existing device (sealed box to the new device's
  ephemeral key, shown as a QR/6-digit code). Devices have their own
  Ed25519 key to sign changes so a compromised device is revocable.
- All crypto via `libsodium`-compatible primitives from the RustCrypto
  crates (`chacha20poly1305`, `x25519-dalek`, `ed25519-dalek`, `argon2`,
  `blake3` for content hashes). No home-grown constructions.

### What the server verifies

The coordinator cannot read change entries but it enforces:

- entries are signed by a device key belonging to an album member with the
  right role (owner / contributor / viewer);
- blob uploads carry a valid **upload grant** (§8) for the album owner's
  storage and quota;
- monotonic sequence numbers per album feed.

### Metadata and search under E2EE

Metadata (EXIF, geo, captions, face embeddings, CLIP embeddings, user tags)
lives in an encrypted **metadata blob** per asset, encrypted with `K_asset`.
Clients maintain a local plaintext index (SQLite via `rusqlite` inside the
core; `sqlite-vec` for embeddings). Because a member of an album holds the
wrapped `K_asset`, they can decrypt the metadata and index shared assets
exactly like their own; that is what delivers R1/R4 "search across shared
albums".

---

## 5. Data model

Server-side (Postgres, coordinator). Fields marked `enc` are opaque
ciphertext to the server.

```
Account        id, email_hash, created_at, plan, quota_bytes, used_bytes
KeyBundle      account_id, kek_params, mk_wrapped_by_kek(enc), mk_wrapped_by_recovery(enc),
               identity_pub, signing_pub, identity_priv(enc), signing_priv(enc)
Device         id, account_id, name, signing_pub, last_seen, revoked_at
StorageNode    id, owner_account_id, name, endpoint_urls[], tailnet_url?, capacity_bytes,
               used_bytes, backend_kind, last_heartbeat, pubkey
Album          id, owner_account_id, created_at, storage_node_id, kind (manual|smart|builtin)
AlbumMember    album_id, account_id, role, ak_sealed(enc), added_by_device, seq
Feed           album_id, seq (monotonic), device_id, signature, payload(enc)   -- append-only
Blob           id (blake3 of ciphertext), size, storage_node_id, album_id (for quota), ref_count
UploadGrant    id, album_id, storage_node_id, grantee_account_id, max_bytes, expires_at, sig
```

Client-side (SQLite inside `photos-core`), plaintext after decryption:

```
Asset          id, k_asset, content_hash (blake3 of plaintext, unique), kind (photo|video|live),
               captured_at, width, height, duration, camera, geo, original_blob, preview_blob,
               thumb_blob, metadata_blob, is_own, eviction_state
LocalCopy      asset_id, device_id, local_uri            -- one asset may exist as several files
AssetMeta      asset_id, perceptual_hash (u64), exif(json), caption, tags[],
               faces[] (embedding, box, person_id?), clip_embedding (vec)
Album          id, name, kind, rule(json)?, ak, owner, my_role, parent_album_id?, sort
AlbumAsset     album_id, asset_id, wrapped_key, added_by, added_at
Person         id, name, representative_face, embedding_centroid
FeedCursor     album_id, last_seq
Job tables     upload_jobs, download_jobs, evict_jobs (persisted state-machine state)
```

**Everything that syncs is a change entry in an album feed.** A user's whole
library is itself an album (`builtin:library`, never shared) so there is a
single sync mechanism. Feed payloads are encrypted with `AK` and are one of:

```
AssetAdded { asset_id, wrapped_key, blob refs, captured_at }
AssetRemoved { asset_id }
MetaPatched { asset_id, field, value, hlc }          -- last-writer-wins per field
AlbumRenamed { name, hlc }
RuleChanged { rule, hlc }                             -- smart albums
MemberAdded { account_id, role, ak_sealed }           -- owner-signed
MemberRemoved { account_id }                          -- owner-signed, triggers rotation
KeyRotated { new_ak_sealed_for_each_member }
ChildAlbumLinked { album_id }                          -- nesting (R1)
```

Conflict rules are simple because the data is simple: sets are add-wins,
scalar fields are LWW by hybrid logical clock, blobs are immutable and
content-addressed. No CRDT library needed.

### Deduplication

E2EE decides where dedup can happen: **on the device, never on the server.**
`BlobId` is BLAKE3 of the *ciphertext* under a fresh `K_asset`, so the same
photo uploaded twice produces two unrelated blobs and the coordinator has
nothing to match on. That is deliberate (see below). Types live in
`photos-core::asset`.

| Level | Key | When | Action |
|-------|-----|------|--------|
| Exact | `ContentHash` = BLAKE3(plaintext original) | `UploadJob::Hashing`, before any encryption | Runtime looks the hash up in the local index. Hit → `Event::Duplicate` → terminal `Deduplicated`; `Effect::LinkDuplicate` records the file as another `LocalCopy` of the existing asset and adds that asset to the target album. Nothing is encrypted or uploaded. |
| Near | `PerceptualHash` (64-bit dHash/pHash; poster frame for video) | ML indexing pass, same place faces/CLIP run | Stored in the encrypted `AssetMeta`. Pairs within Hamming distance ≤ 10 (plus a duration check for video) feed the built-in `Duplicates` smart album. Suggest-only: the user picks what to keep; we never merge or delete automatically. |
| Shared | `asset_id` / `ContentHash` | Backfilling an album feed | Shared assets are references, not copies (§6). If a member already owns a byte-identical asset, the client links the two records locally so the timeline shows one item. |

Why this covers the real cases: the bulk of duplicates come from importing
the same photo via several paths (camera roll + iCloud export + Google
Takeout — phase 6 importers) — exact matches. Bursts, edits and messenger
re-encodes are near matches and inherently need a human decision.

What we intentionally do **not** do:

- **Cross-account server-side dedup.** The only way to get it under E2EE is
  convergent encryption (`K_asset` derived from the content hash), which
  leaks "does *anyone* on this server have this file" to the operator and
  enables confirmation-of-file attacks. Storage is cheaper than that
  trade-off. Hosted-tier costs are handled by quotas, not dedup.
- **Chunk-level dedup across assets.** Same reason: chunks are encrypted
  under per-asset keys, so identical plaintext chunks never share a
  ciphertext.

The `ContentHash` is also what makes re-scans idempotent: reinstalling the
app or adding a device re-hashes the camera roll and finds every asset
already in the library without re-uploading.

---

## 6. Sync protocol

- Each album feed is an append-only log with server-assigned `seq`.
- Client keeps `FeedCursor` per album; `GET /albums/{id}/feed?since=seq`
  (long-poll or WebSocket push for liveness).
- Client pushes `POST /albums/{id}/feed` with signed, encrypted entries. The
  server assigns `seq`, rejects if the device is not a member with rights.
- Blobs are uploaded **before** the entry that references them is pushed
  (entry commit = "blob is durable"). Blob upload is resumable per 4 MiB
  chunk; the storage node reports which chunks it has.
- Because feeds are per album, sharing an album with someone is just
  granting them the feed + the sealed `AK`; their client backfills from
  `seq=0`.
- Optimize-storage: local originals are evicted only when the asset's
  original blob is confirmed on a storage node with the album's required
  replication factor (default 1 for BYO, 2 for hosted).

---

## 7. State machines and actors (A4)

All domain logic in `photos-core` follows one pattern:

```rust
pub trait StateMachine {
    type State;
    type Event;
    type Effect;
    fn transition(state: Self::State, event: Self::Event) -> (Self::State, Vec<Self::Effect>);
}
```

`transition` is pure: no I/O, no clocks, no randomness (those are inputs via
events). The **actor runtime** (`core::actor`) gives each machine a mailbox
(tokio `mpsc`), persists `State` to SQLite after every transition, executes
`Effect`s (HTTP, disk, crypto, timers) and feeds results back as events. This
gives:

- deterministic unit tests: `assert_eq!(transition(s, e), (s2, effects))`;
- crash safety: on restart every actor is rehydrated from its persisted state;
- one implementation for native, web and server.

Supervision tree (client):

```
Supervisor
├── AccountSession          Locked → Unlocking → Unlocked → (Revoked)
├── FeedSync[album]         Idle → Pulling(since) → Applying → Idle | Backoff(n)
│                           Pushing(pending) → Acked
├── LibraryScanner          Idle → Scanning(cursor) → Diffing → Idle    (camera roll)
├── UploadQueue
│   └── UploadJob[asset]    Discovered → Hashing → Encrypting(chunk) → RequestingGrant
│                           → Uploading(chunk, attempt) → Committing → Done | Failed(reason)
│                           Hashing → Deduplicated   (content hash already in library)
├── DownloadQueue
│   └── DownloadJob[blob]   Queued → Fetching(chunk) → Decrypting → Done
├── StorageEvictor          Idle → Evaluating → Evicting(batch) → Idle   (optimize storage)
├── SmartAlbumMaterializer[album]  Idle → Evaluating(rule) → Diffing → Publishing → Idle
├── KeyRotator[album]       Idle → Rotating → ReWrapping(progress) → Publishing → Idle
├── MlIndexer               Idle → PHash(batch) → Faces(batch) → Clip(batch) → Clustering → Idle
└── Importer[source]        Idle → Enumerating → Ingesting(item) → Done  (iCloud/Takeout)
```

Server-side (coordinator and storage node) reuse the same runtime for:

```
GrantIssuer                 per upload grant lifecycle
NodeHealth[node]            Unknown → Healthy → Degraded → Offline
Replication[blob]           Single → Replicating(target) → Replicated
BlobGc                      RefCounted → Tombstoned → Deleted
```

The scaffold in this repo implements the trait, the runtime skeleton and a
fully tested `UploadJob` machine as the reference example.

**Actor ↔ UI bridge.** `photos-core` exposes to TypeScript exactly two calls:
`dispatch(event)` and `subscribe(selector) -> stream<State>`. The RN app is
therefore a pure function of core state, and web/native/testing all drive the
same surface.

---

## 8. Storage nodes (A5)

`photos-storage-node` is a single static binary (`docker run` or a bare
executable on a NAS / Raspberry Pi / Mac mini) with:

- `PUT /blobs/{id}/chunks/{n}`, `HEAD /blobs/{id}` (which chunks are
  present), `GET /blobs/{id}` (range requests), `DELETE /blobs/{id}`.
- Backends behind a `trait Backend`: `LocalDisk` (v1), `S3Compatible`
  (v1; covers AWS, R2, B2, MinIO, Garage), `Mirror` (write to two backends).
- **Registration**: node generates a keypair, the user pairs it with their
  account via a one-time code in the app; the coordinator stores the node's
  public key and endpoints and receives heartbeats with capacity/usage.
- **Authorization**: clients never get node credentials. The coordinator
  issues short-lived **upload grants** (signed JWTs: album, node, max bytes,
  expiry) that the node verifies with the coordinator's public key. This is
  how "contributor uploads land on the owner's storage and quota" (R3) works
  without the owner being online.
- **Tailscale**: the node advertises its tailnet hostname alongside any
  public URL. Clients that are on the same tailnet (detected by successfully
  reaching the `100.x` / MagicDNS address) connect directly; others use the
  public URL, or Tailscale Funnel if the user enabled it, or fall back to the
  coordinator's **relay** endpoint (opt-in, metered on hosted). No Tailscale
  SDK dependency in v1; just address selection. `tsnet` embedding is a
  possible v2 for zero-config.
- Album → node placement is chosen by the owner per album (default: the
  owner's default node). Hosted storage is the same binary with `S3` backend
  pointed at our bucket.

---

## 9. Client platform notes

| Concern | iOS | Android | Web |
|---------|-----|---------|-----|
| Camera roll | `PHPhotoLibrary` change observer via `expo-media-library` + custom module for incremental change tokens | `MediaStore` content observer | Folder picker / drag-drop only |
| Background upload | Encrypt to a temp file, hand to background `URLSession`; `BGProcessingTask` for encrypt/index work | `WorkManager` foreground service for encrypt+upload | Only while tab open (Service Worker Background Fetch where available) |
| Core runtime | UniFFI Swift bindings in an Expo Module | UniFFI Kotlin bindings in an Expo Module | WASM in a Web Worker; OPFS for cache |
| Local DB | SQLite via `rusqlite` (bundled) | same | `sqlite-wasm` with OPFS VFS |
| ML | Core ML (face detect/embed, CLIP image+text) | TFLite / NNAPI | `onnxruntime-web` (WebGPU); text-side CLIP for search first, image indexing optional |
| Thumbnails | `PHImageManager` | `ImageDecoder` | `createImageBitmap` |

Models: face detection + ArcFace-style embeddings; CLIP ViT-B/32 (or
MobileCLIP) for semantic search. Embeddings are stored in the encrypted
metadata blob so indexing happens once per asset, on whichever member device
gets there first, and every other member benefits. Person *names* are
per-account (a face cluster you name is your label; sharing labels is a
future opt-in).

---

## 10. Hosted tiers and billing (A2)

| Tier | What we run | What the user runs | Charge for |
|------|-------------|--------------------|------------|
| Hosted | coordinator + storage nodes | nothing | storage (GB-month), egress above a cap |
| Hosted-lite | coordinator | their own storage node(s) | small flat fee per account (coordinator + relay minutes) |
| Self-hosted | nothing | everything | nothing (Apache-2.0) |

Quota accounting is always against the **album owner** (R3). Billing is a
feature-flagged module in the coordinator (`--features billing`, Stripe);
self-hosters never see it. Plans/quotas are ordinary rows so a self-hoster
can also run a family "plan".

---

## 11. Delivery phases

Estimates are Devin sessions, assuming PR-per-phase and review in between.

| Phase | Scope | Sessions |
|-------|-------|----------|
| 0 (this PR) | Architecture, monorepo scaffold, Rust core with state machine runtime + `UploadJob`, server + storage node skeletons, Expo app skeleton, CI | 1 |
| 1 | Accounts, keys, device pairing; `builtin:library` feed; camera roll scan; encrypted upload to a LocalDisk storage node; timeline UI | 2 |
| 2 | Albums, sharing (sealed AK), roles, upload grants (owner pays), nested albums, key rotation on member removal | 2 |
| 3 | Storage node hardening: S3 backend, pairing flow, Tailscale address selection, relay; optimize-storage evictor | 1–2 |
| 4 | Video (chunked streaming decrypt, thumbnails), Live Photos | 1 |
| 5 | On-device ML: faces + people, CLIP search, perceptual hashes; smart albums + materializer, built-in `Duplicates` album | 2 |
| 6 | Importers (iCloud export folder, Google Takeout), export/backup | 1 |
| 7 | Hosted: billing module, quotas, ops (metrics, backups), app store builds | 1–2 |

External waits not counted: Apple developer account / App Store review,
Play Console, Stripe account, domains.

---

## 12. Risks and open questions

1. **`react-strict-dom` maturity.** Mitigation in §3; `packages/ui` is the
   only place that imports it.
2. **iOS background execution budget.** Encryption before upload means we
   cannot hand the raw file to `URLSession` directly. Mitigation: encrypt
   in a `BGProcessingTask`, upload the ciphertext with background
   `URLSession`; also encrypt+upload opportunistically while foregrounded.
3. **Smart albums need an online owner device** to materialize new matches
   for members. Acceptable for v1 (same as iCloud Shared Albums requiring
   the sharer's device); document it. Long-term option: a user-run
   "headless device" mode in the storage node binary.
4. **Web ML cost.** Ship text-side CLIP search on web first; image indexing
   on web is optional/opt-in.
5. **Member removal cannot revoke already-downloaded content.** Inherent to
   E2EE; the UI must say so.
6. **Metadata leak to the coordinator** (sizes, timing, social graph). Same
   as comparable E2EE products; documented in the threat model.
7. Open: name, domain, whether the storage node should embed `tsnet` in v1
   (currently no), whether people/face *labels* are shareable (currently no).

---

## 13. Repository layout

```
photos/
├── apps/
│   └── app/                  Expo app: iOS, Android, web
├── packages/
│   ├── core/                 TS facade over the Rust core (dispatch/subscribe), picks native or wasm
│   ├── core-native/          Expo Module wrapping UniFFI Swift/Kotlin bindings
│   ├── core-wasm/            wasm-bindgen output + worker glue
│   ├── ui/                   StyleX / react-strict-dom component library
│   └── config/               shared tsconfig, biome config
├── crates/
│   ├── core/                 photos-core: state machines, actor runtime, crypto, sync, index
│   ├── core-uniffi/          UniFFI surface for mobile
│   ├── core-wasm/            wasm-bindgen surface for web
│   ├── protocol/             wire types shared by clients, coordinator and storage node
│   ├── server/               coordinator (axum + Postgres)
│   └── storage-node/         blob store binary
├── docs/
├── .github/workflows/
├── Cargo.toml                Rust workspace
├── package.json              Bun workspaces
└── LICENSE                   Apache-2.0
```
