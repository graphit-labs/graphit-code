import { nativeRecordRead } from './nativeRecordRead'
export type LibraryKind = 'ast' | 'knowledge' | 'language'
export interface InstalledLibrary {
  artifact_type: LibraryKind; id: string; version: string; producer: string
  target: 'global_native'; origin: 'HubCache' | 'Resources' | 'LocalLink'
  provider: string | null; alias: string | null; entry_name: string | null
  display: { name: string; description: string | null; tags: string[] } | null
}
export interface InstalledLibraryPage {
  scope: 'global_installation'; entries: InstalledLibrary[]; next_offset: number | null
}
export interface LibraryQuery { artifact_type?: LibraryKind; id?: string; version?: string; offset: number; limit: number }
export const libraryKey = (entry: InstalledLibrary) => JSON.stringify([entry.artifact_type, entry.id, entry.version, entry.producer, entry.target, entry.origin, entry.provider, entry.alias, entry.entry_name])
export async function installedLibraries(query: LibraryQuery, signal?: AbortSignal): Promise<InstalledLibraryPage> {
  const page = await nativeRecordRead<InstalledLibraryPage>('/hub/installed/native', { scope: 'global_installation', ...query }, signal)
  if (page.scope !== 'global_installation' || !Array.isArray(page.entries) || (page.next_offset !== null && (!Number.isSafeInteger(page.next_offset) || page.next_offset <= query.offset))) throw new Error('Invalid installed library response')
  return page
}
