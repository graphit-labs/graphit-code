import { afterEach, expect, it, vi } from 'vitest'
import { hubNative, installedFileKey, type InstalledFile, type InstalledQuery } from './hubNative'

const query: InstalledQuery = { scope: { kind: 'project', project_id: 'project', project_version: 'branch' }, binding: 'codex', offset: 0, limit: 20 }
const entry: InstalledFile = { artifact_type: 'skill', id: 'local/reviewer', version: 'release', producer: 'producer', target: { kind: 'native_binding', binding: 'codex' }, source_origin: 'HubCache', origin: { transport: 'broker', provider: 'corporate' }, alias: null, entry_name: null, display: null }
function response(value: unknown) { vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ status: 'ok', scope: query.scope, entries: [value], next_offset: null }), { status: 200 }))) }
afterEach(() => vi.unstubAllGlobals())

it.each([
  { source_origin: 'HubCache', origin: { transport: 'broker', provider: 'corporate' } },
  { source_origin: 'HubCache', origin: { transport: 'standalone', provider: 'files', profile: 'production' } },
  { source_origin: 'HubCache', origin: null },
  { source_origin: 'Resources', origin: null },
  { source_origin: 'LocalLink', origin: null },
])('installed SourceOrigin accepts exact actual provenance %j without deriving it from remote origin', async provenance => {
  const value = { ...entry, ...provenance }
  response(value)
  await expect(hubNative.installed(query)).resolves.toMatchObject({ entries: [value] })
})

it.each([undefined, null, '', 'local', 'resources', 'Hubcache', 'Link', 'FutureOrigin', ['LocalLink'], { LocalLink: true }])('installed SourceOrigin rejects missing/null/unknown discriminator %j', async source_origin => {
  response({ ...entry, source_origin, origin: null })
  await expect(hubNative.installed(query)).rejects.toThrow('Invalid installed FILE page')
})

it.each(['Resources', 'LocalLink'])('installed SourceOrigin rejects contradictory %s with a remote transport', async source_origin => {
  response({ ...entry, source_origin })
  await expect(hubNative.installed(query)).rejects.toThrow('Invalid installed FILE page')
})

it.each([undefined, 'broker', [], { transport: 'future', provider: 'corporate' }, { transport: 'broker' }, { transport: 'standalone', provider: 'corporate', profile: 3 }])('installed SourceOrigin rejects malformed remote origin %j instead of adding capabilities', async origin => {
  response({ ...entry, origin })
  await expect(hubNative.installed(query)).rejects.toThrow('Invalid installed FILE page')
})

it('installed SourceOrigin rejects an invalid entry after valid entries without partial success', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ status: 'ok', scope: query.scope, entries: [entry, { ...entry, source_origin: null }], next_offset: null }), { status: 200 })))
  await expect(hubNative.installed(query)).rejects.toThrow('Invalid installed FILE page')
})

it('installed selection identity distinguishes Resources, LocalLink and HubCache with identical null remote origin', () => {
  const keys = ['Resources', 'LocalLink', 'HubCache'].map(source_origin => installedFileKey({ ...entry, source_origin: source_origin as InstalledFile['source_origin'], origin: null }))
  expect(new Set(keys).size).toBe(3)
})
