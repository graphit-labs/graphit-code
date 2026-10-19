import { afterEach, expect, it, vi } from 'vitest'
import { hubNative, type AuthoredQuery, type AuthoredSelection } from './hubNative'

const project = { project_id: '01KSH1CRFFG8Z74B5ZS78WW808', project_version: 'feature/source' }
const selected: AuthoredSelection = { ...project, artifact_type: 'skill', id: 'vendor/reviewer', version: 'artifact/one' }
const entry = { artifact_type: selected.artifact_type, id: selected.id, version: selected.version, name: null }
const page = () => ({ status: 'ok', ...project, entries: [entry], next_cursor: null })
const respond = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status })
afterEach(() => vi.unstubAllGlobals())

it('authored API captures exact list selection before await and uses the existing same-origin POST transport', async () => {
  let finish!: (response: Response) => void
  const fetcher = vi.fn(() => new Promise<Response>(resolve => { finish = resolve }))
  vi.stubGlobal('fetch', fetcher)
  const query: AuthoredQuery = { ...project, artifact_type: 'skill', id: selected.id, version: selected.version, limit: 20 }
  const signal = new AbortController().signal
  const work = hubNative.authoredList(query, signal)
  query.project_version = 'changed'; query.id = 'other'
  finish(respond(page()))
  await expect(work).resolves.toMatchObject(page())
  const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit]
  expect(url).toMatch(/\/hub\/authored\/list$/)
  expect(init).toMatchObject({ method: 'POST', credentials: 'same-origin', signal })
  expect(JSON.parse(init.body as string)).toEqual({ ...project, artifact_type: 'skill', id: selected.id, version: selected.version, limit: 20 })
  expect(new Headers(init.headers).get('X-Graphit-Request')).toBe('ui')
})

it.each([
  { ...page(), project_id: 'other-project' },
  { ...page(), project_version: 'other-version' },
  { ...page(), entries: [{ ...entry, artifact_type: 'invented' }] },
  { ...page(), entries: [{ ...entry, name: 'Invented descriptor' }] },
  { ...page(), entries: [{ artifact_type: 'skill', id: selected.id, version: selected.version }] },
  { ...page(), next_cursor: 'same-cursor' },
])('authored API rejects crossed or unsupported list data without normalizing it: %j', async body => {
  vi.stubGlobal('fetch', vi.fn(async () => respond(body)))
  await expect(hubNative.authoredList({ ...project, cursor: 'same-cursor', limit: 20 })).rejects.toThrow('Invalid authored resource page')
})

it.each(['ast', 'knowledge'] as const)('authored API retains %s resources for reading without inventing FILE publication metadata', async artifact_type => {
  vi.stubGlobal('fetch', vi.fn(async () => respond({ ...page(), entries: [{ ...entry, artifact_type }] })))
  await expect(hubNative.authoredList({ ...project, artifact_type, limit: 20 })).resolves.toMatchObject({ entries: [{ artifact_type, name: null }] })
})

it('authored API preserves native byte counts and independent tree pagination with the exact Resources source', async () => {
  const fetcher = vi.fn(async () => new Response(JSON.stringify({ status: 'ok', ...selected, source: 'Resources', metadata: null, entries: [{ path: 'large.dat', kind: 'File', bytes: 'replace-native-count' }], next_offset: 20 }).replace('"replace-native-count"', '9007199254740993')))
  vi.stubGlobal('fetch', fetcher)
  const input = { ...selected, offset: 0, limit: 20 }
  const result = await hubNative.authoredReview(input)
  expect(result.entries[0].bytes).toBe(9007199254740993n)
  expect(JSON.parse((fetcher.mock.calls[0] as unknown as [string, RequestInit])[1].body as string)).toEqual(input)
  expect((fetcher.mock.calls[0] as unknown as [string])[0]).toMatch(/\/hub\/authored\/review$/)
})

it.each([
  { source: 'HubCache' }, { version: 'other-artifact-version' }, { next_offset: 0 },
  { metadata: {} }, { entries: [{ path: 'file', kind: 'Symlink', bytes: 0 }] },
  { entries: [{ path: 'file', kind: 'File', bytes: -1 }] },
  ...['/host/absolute', '../escape', 'docs/../escape'].map(path => ({ entries: [{ path, kind: 'File', bytes: 0 }] })),
])('authored API rejects mismatched source, selection or malformed tree pages: %j', async delta => {
  vi.stubGlobal('fetch', vi.fn(async () => respond({ status: 'ok', ...selected, source: 'Resources', metadata: null, entries: [], next_offset: null, ...delta })))
  await expect(hubNative.authoredReview({ ...selected, offset: 0, limit: 20 })).rejects.toThrow('Invalid authored resource review')
})

it('authored API preserves successful empty text and exact tree-relative content identity', async () => {
  const fetcher = vi.fn(async () => respond({ status: 'ok', ...selected, content: { path: 'empty.md', content: '', bytes: 0 } }))
  vi.stubGlobal('fetch', fetcher)
  await expect(hubNative.authoredContent({ ...selected, path: 'empty.md' })).resolves.toMatchObject({ content: { content: '', bytes: 0 } })
  const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit]
  expect(url).toMatch(/\/hub\/authored\/content$/)
  expect(JSON.parse(init.body as string)).toEqual({ ...selected, path: 'empty.md' })
  fetcher.mockImplementation(async () => respond({ status: 'ok', ...selected, content: { path: 'other.md', content: '', bytes: 0 } }))
  await expect(hubNative.authoredContent({ ...selected, path: 'empty.md' })).rejects.toThrow('Invalid authored resource content')
})

it.each([[404, 'authored_file_not_found'], [415, 'authored_content_not_text'], [413, 'authored_too_large'], [401, 'unauthorized'], [403, 'permission_denied']] as const)('authored API preserves HTTP%s/%s as an error, not an empty resource', async (status, code) => {
  vi.stubGlobal('fetch', vi.fn(async () => respond({ status: 'error', error: { code } }, status)))
  await expect(hubNative.authoredContent({ ...selected, path: 'SKILL.md' })).rejects.toMatchObject({ status, code, request: { ...selected, path: 'SKILL.md' } })
})

it.each(['/host/absolute', '../escape', 'docs/../escape'])('authored API refuses a non-relative content selection without rewriting it or sending a request: %s', async path => {
  const fetcher = vi.fn(); vi.stubGlobal('fetch', fetcher)
  await expect(hubNative.authoredContent({ ...selected, path })).rejects.toThrow('Invalid authored resource path')
  expect(fetcher).not.toHaveBeenCalled()
})


it('authored LIST decodes the actual emitted HTTP200 nonempty entry with name:null and no metadata field', async () => {
  const emitted = { status: 'ok', ...project, entries: [{ artifact_type: 'skill', id: 'vendor/reviewer', version: 'artifact/one', name: null }], next_cursor: null }
  vi.stubGlobal('fetch', vi.fn(async () => respond(emitted)))
  const actual = await hubNative.authoredList({ ...project, limit: 20 })
  expect(actual).toEqual(emitted)
  expect(actual.entries[0]).toHaveProperty('name', null)
  expect(actual.entries[0]).not.toHaveProperty('metadata')
})

it.each([undefined, 'display name', 0, false, {}, []])('authored LIST rejects absent or non-null name %j without substituting metadata', async name => {
  vi.stubGlobal('fetch', vi.fn(async () => respond({ ...page(), entries: [{ artifact_type: selected.artifact_type, id: selected.id, version: selected.version, name, metadata: null }] })))
  await expect(hubNative.authoredList({ ...project, limit: 20 })).rejects.toThrow('Invalid authored resource page')
})

it('authored REVIEW keeps required metadata:null independently of LIST name:null', async () => {
  const emitted = { status: 'ok', ...selected, source: 'Resources', metadata: null, entries: [], next_offset: null }
  const fetcher = vi.fn(async () => respond(emitted)); vi.stubGlobal('fetch', fetcher)
  await expect(hubNative.authoredReview({ ...selected, limit: 20 })).resolves.toEqual(emitted)
  fetcher.mockImplementation(async () => respond({ status: 'ok', ...selected, source: 'Resources', name: null, entries: [], next_offset: null }))
  await expect(hubNative.authoredReview({ ...selected, limit: 20 })).rejects.toThrow('Invalid authored resource review')
})
