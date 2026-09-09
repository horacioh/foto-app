import type { CoreBackend } from '@photos/core'
import { requireNativeModule } from 'expo-modules-core'

/**
 * Shape of the Expo Module defined in `ios/PhotosCoreModule.swift` and
 * `android/.../PhotosCoreModule.kt`. Both delegate to the UniFFI-generated
 * `Core` class from `crates/core-uniffi`.
 */
interface PhotosCoreNativeModule {
  version(): string
  dispatch(eventJson: string): void
  stateJson(): string
}

export function createNativeBackend(): CoreBackend {
  const native = requireNativeModule<PhotosCoreNativeModule>('PhotosCore')
  return {
    version: () => native.version(),
    dispatch: (eventJson) => native.dispatch(eventJson),
    stateJson: () => native.stateJson(),
  }
}
