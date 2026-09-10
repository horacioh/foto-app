import { describe, expect, test } from 'bun:test'
import { CoreClient } from './client.ts'
import { createMemoryBackend } from './memory.ts'

describe('CoreClient', () => {
  test('dispatches events and updates the snapshot', () => {
    const client = new CoreClient(createMemoryBackend('1.2.3'))
    expect(client.version).toBe('1.2.3')
    expect(client.getSnapshot()).toEqual({ version: '1.2.3', pending_events: 0 })

    client.dispatch({ type: 'sync/now' })
    client.dispatch({ type: 'album/create', name: 'Trip' })
    expect(client.getSnapshot().pending_events).toBe(2)
  })

  test('notifies subscribers and stops after unsubscribe', () => {
    const client = new CoreClient(createMemoryBackend())
    let calls = 0
    const unsubscribe = client.subscribe(() => {
      calls += 1
    })
    client.dispatch({ type: 'sync/now' })
    unsubscribe()
    client.dispatch({ type: 'sync/now' })
    expect(calls).toBe(1)
  })

  test('rejects malformed snapshots from the backend', () => {
    expect(
      () =>
        new CoreClient({
          version: () => 'x',
          dispatch: () => {},
          stateJson: () => '{"nope":true}',
        }),
    ).toThrow(/invalid snapshot/)
  })
})
