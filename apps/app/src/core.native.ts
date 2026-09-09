import type { CoreBackend } from '@photos/core'
import { createMemoryBackend } from '@photos/core/memory'
import { createNativeBackend } from '@photos/core-native'

/** iOS/Android: UniFFI-backed core. `EXPO_PUBLIC_CORE_BACKEND=memory` skips the native build. */
export async function loadCore(): Promise<CoreBackend> {
  if (process.env.EXPO_PUBLIC_CORE_BACKEND === 'memory') return createMemoryBackend()
  return createNativeBackend()
}
