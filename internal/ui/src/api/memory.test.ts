import { afterEach, describe, expect, it, vi } from 'vitest'
import { memoryApi } from './memory'

describe('Memory API project scope', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('uses project_id for Hub project memory and keeps user memory unbound', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ results: [], total: 0, types: [], tags: [] }) })
    vi.stubGlobal('fetch', fetchMock)

    await memoryApi.list({ scope: 'project', projectId: '01HUB' })
    await memoryApi.list({ scope: 'user' })

    expect(fetchMock.mock.calls[0][0]).toBe('/api/memories?project_id=01HUB&scope=project')
    expect(fetchMock.mock.calls[1][0]).toBe('/api/memories?scope=user')
  })
})
