import { afterEach, expect, it, vi } from 'vitest'
import { fetchPage, fetchPages, searchWiki } from './wiki'

afterEach(() => vi.unstubAllGlobals())

it('reads only an exact published Knowledge context for a Hub project', async () => {
  const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => [] })
  vi.stubGlobal('fetch', fetchMock)
  const scope = { projectId: '01HUB', context: 'knowledge-artifact@3.4.0' }

  await fetchPages('', scope)
  await fetchPage('', 'architecture.md', scope)
  await searchWiki('', 'boundaries', scope)

  expect(fetchMock.mock.calls.map(call => call[0])).toEqual([
    '/api/wiki/pages?project_id=01HUB&context=knowledge-artifact%403.4.0',
    '/api/wiki/page?project_id=01HUB&context=knowledge-artifact%403.4.0&path=architecture.md',
    '/api/wiki/search?project_id=01HUB&context=knowledge-artifact%403.4.0&q=boundaries',
  ])
})
