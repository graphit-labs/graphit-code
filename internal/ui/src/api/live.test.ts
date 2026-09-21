import { afterEach, expect, it, vi } from 'vitest'
import { createLiveSession } from './live'

afterEach(() => vi.unstubAllGlobals())

it('preserves the selected CLI diagnostic returned by session creation', async () => {
  const diagnostic = 'agent "codex" requires CLI "codex" on PATH; install and authenticate that CLI'
  const fetchMock = vi.fn().mockResolvedValue(new Response(`${diagnostic}\n`, { status: 500 }))
  vi.stubGlobal('fetch', fetchMock)

  await expect(createLiveSession({ agent: 'codex', prompt: 'investigate' }))
    .rejects.toMatchObject({ name: 'LiveError', status: 500, message: diagnostic })
  expect(fetchMock).toHaveBeenCalledWith('/api/live/sessions', expect.objectContaining({
    method: 'POST', body: JSON.stringify({ agent: 'codex', prompt: 'investigate' }),
  }))
})
