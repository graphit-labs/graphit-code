import { describe, expect, it } from 'vitest'
import { projectRecordHref, requestedRecordSelection, userMemoryHref, importedKnowledgeHref, requestedImportedKnowledgeSource, importedSourceKey, projectReadModule, requestedAstContext, importedAstHref, requestedImportedAstSource } from './nativeRecordNavigation'

const locator = { project_id: 'project1', project_version: 'literal / version' }
const selection = { ...locator, project_dir: '/ÿp' }

describe('native record navigation', () => {
  it('retains the exact imported tuple, alias and literal slug without PROJECT selection or id@version parsing', () => {
    const source = { kind: 'imported_knowledge' as const, artifact: { id: 'literal@id', version: ' exact / version ', context_alias: 'alias/@literal' } }
    const url = new URL(importedKnowledgeHref(source, 'literal /?#é'), 'https://fixture.invalid')
    expect(requestedImportedKnowledgeSource(url.search)).toEqual(source)
    expect(url.searchParams.get('page')).toBe('literal /?#é')
    expect(url.searchParams.has('selection')).toBe(false)
    expect(projectReadModule(url.pathname, url.search)).toBeNull()
    expect(importedSourceKey(source)).not.toBe(importedSourceKey({ ...source, artifact: { ...source.artifact, context_alias: 'other' } }))
    for (const malformed of [{ ...source, extra: true }, { ...source, artifact: { ...source.artifact, context_alias: null } }, { kind: 'imported_knowledge', artifact: { id: 'id', version: 1 } }]) {
      expect(() => requestedImportedKnowledgeSource('?' + new URLSearchParams({ library: 'imported', source: JSON.stringify(malformed) }))).toThrow('invalid')
    }
    expect(() => requestedImportedKnowledgeSource('?library=imported&source=%7B&source=%7B')).toThrow('ambiguous')
    expect(projectReadModule('/knowledge/explorer', '?library=unknown')).toBeNull()
    expect(projectReadModule('/memory/explorer/project', '')).toBe('memory')
    expect(projectReadModule('/knowledge/explorer', '')).toBe('knowledge')
  })
  it('preserves portable project locators and literal IDs in all PROJECT reader links', () => {
    const id = 'literal /?#é'
    for (const kind of ['task', 'session', 'memory', 'knowledge'] as const) {
      const url = new URL(projectRecordHref(kind, id, selection), 'https://fixture.invalid')
      expect(requestedRecordSelection(url.search)).toEqual(locator)
      if (kind === 'knowledge') expect(url.searchParams.get('page')).toBe(id)
      else expect(decodeURIComponent(url.pathname.split('/').at(-1)!)).toBe(id)
    }
    const mirror = { ...selection, project_dir: "/m" }
    expect(projectRecordHref('task', 'sameID', selection)).toBe(projectRecordHref('task', 'sameID', mirror))
  })

  it('drops Windows checkout paths and validates a closed locator without normalizing versions', () => {
    const windows = { ...selection, project_dir: 'C:\\projeto com espaço\\ação Ω', project_version: '  exact  ' }
    expect(requestedRecordSelection(new URL(projectRecordHref('memory', 'id', windows), 'https://fixture.invalid').search)).toEqual({ ...locator, project_version: windows.project_version })
    expect(requestedRecordSelection('')).toBeNull()
    for (const value of [selection, { ...locator, other: true }, { ...locator, project_version: '\ud800' }, { ...locator, project_id: 'bad\0id' }]) {
      expect(() => requestedRecordSelection('?' + new URLSearchParams({ selection: JSON.stringify(value) }))).toThrow('invalid')
    }
    expect(() => requestedRecordSelection('?selection=%7B&selection=%7B')).toThrow('ambiguous')
  })

  it('binds USER navigation only to the observed literal scope and sends no PROJECT selection', () => {
    const scope = '["fixture-installation","https://issuer.invalid","fixture subject"]'
    const url = new URL(userMemoryHref('literal /id', scope), 'https://fixture.invalid')
    expect(url.searchParams.get('scope_id')).toBe(scope)
    expect(url.searchParams.has('selection')).toBe(false)
    expect(url.pathname).toBe('/memory/explorer/user/literal%20%2Fid')
  })
})

it('decodes the observed AST URI segment once and keeps PROJECT admission independent of imports', () => {
 const id = 'library%2Fliteral@ exact version ';
 expect(requestedAstContext('/ast/explorer/'+encodeURIComponent(id))).toBe(id);
 expect(projectReadModule('/ast/explorer/%70roject','')).toBe('ast');
 expect(projectReadModule('/ast/explorer/project','?library=imported')).toBeNull();
 expect(projectReadModule('/ast/contexts','')).toBe('ast');
 expect(projectReadModule('/ast/explorer','')).toBe('ast');
 expect(()=>requestedAstContext('/ast/explorer/%')).toThrow('invalid');
});

it('keeps an observed installed AST key literal and separate from PROJECT', () => {
 const installed = { id: 'literal@id', version: ' exact / version ', context: 'observed/context@tuple', backend: 'native' as const }
 const url = new URL(importedAstHref(installed), 'https://fixture.invalid')
 expect(requestedAstContext(url.pathname)).toBe(installed.context)
 expect(requestedImportedAstSource(url.search)).toEqual({ id: installed.id, version: installed.version })
 expect(url.searchParams.has('selection')).toBe(false)
 expect(projectReadModule(url.pathname, url.search)).toBeNull()
 for (const malformed of [{ id: 'id', version: 1 }, { id: 'id', version: 'v', context: 'alias' }]) expect(() => requestedImportedAstSource('?' + new URLSearchParams({ library: 'imported', source: JSON.stringify(malformed) }))).toThrow('invalid')
 expect(() => requestedImportedAstSource(url.search + '&selection=%7B%7D')).toThrow('PROJECT')
 expect(() => requestedImportedAstSource(url.search + '&source=%7B%7D')).toThrow('ambiguous')
})
