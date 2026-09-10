import type { CoreClient } from '@photos/core'
import { Button, Screen, Text } from '@photos/ui'
import { StatusBar } from 'expo-status-bar'
import { useCore, useCoreSnapshot } from './useCore'

export function App() {
  const core = useCore()
  return (
    <Screen>
      <StatusBar style="light" />
      <Text variant="title">Photos</Text>
      <Text variant="caption">
        Open-source, end-to-end encrypted photo library. Your albums, your storage.
      </Text>
      {core.kind === 'loading' && <Text>Starting core…</Text>}
      {core.kind === 'error' && <Text>Core failed to start: {core.message}</Text>}
      {core.kind === 'ready' && <CorePanel client={core.client} />}
    </Screen>
  )
}

function CorePanel({ client }: { client: CoreClient }) {
  const snapshot = useCoreSnapshot(client)
  return (
    <>
      <Text variant="caption">core v{snapshot.version}</Text>
      <Text>Pending events: {snapshot.pending_events}</Text>
      <Button onPress={() => client.dispatch({ type: 'sync/now' })}>Sync now</Button>
    </>
  )
}
