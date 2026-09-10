package app.photos.core

import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import uniffi.photos_core_ffi.Core
import uniffi.photos_core_ffi.version

// Thin Expo Module over the UniFFI-generated Kotlin bindings
// (src/main/java/uniffi, produced by `bun run --filter @photos/core-native build:rust`).
class PhotosCoreModule : Module() {
  private val core = Core()

  override fun definition() = ModuleDefinition {
    Name("PhotosCore")

    Function("version") { version() }

    Function("dispatch") { eventJson: String -> core.dispatch(eventJson) }

    Function("stateJson") { core.stateJson() }
  }
}
