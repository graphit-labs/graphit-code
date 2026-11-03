import type { AstImportedSource, AstInstalledContext } from './nativeAst'
import type { TaskContext } from './task'
import type { ImportedKnowledgeSource } from './nativeKnowledge'
import type { ProjectReadModule } from './nativeRecordRead'
import { parseNativeJson, stringifyNativeJson } from './nativeJson'

export type ProjectRecordKind = 'task' | 'session' | 'memory' | 'knowledge'
export type ProjectLocator = Pick<TaskContext, 'project_id' | 'project_version'>

export const sameProjectVersion = (left: ProjectLocator | null, right: ProjectLocator) =>
  left?.project_id === right.project_id && left.project_version === right.project_version

export const encodeProjectLocator = ({ project_id, project_version }: ProjectLocator) =>
  stringifyNativeJson({ project_id, project_version })

export const nativeSelectionKey = (selection: TaskContext | null) => stringifyNativeJson(selection && [selection.project_dir, selection.project_id, selection.project_version])
export const importedSourceKey = (source: ImportedKnowledgeSource | null) => stringifyNativeJson(source && [source.artifact.id, source.artifact.version, source.artifact.context_alias ?? null])
export const importedKnowledgeRoute = (search: string) => {
  const params = new URLSearchParams(search), libraries = params.getAll('library')
  return params.has('source') || (libraries.length > 0 && (libraries.length !== 1 || libraries[0] !== 'project'))
}
export const importedKnowledgeLocation = (pathname: string, search: string) => pathname === '/knowledge/contexts' || (pathname.startsWith('/knowledge/explorer') && importedKnowledgeRoute(search))
export const installedHubRoute = (pathname: string) => pathname === '/hub/installed'
export const fileHubRoute = (pathname: string) => pathname === '/hub/registry' || pathname === '/hub/local' || pathname === '/hub/upload'
export const userMemoryRoute = (pathname: string) => pathname === '/memory/explorer/user' || pathname.startsWith('/memory/explorer/user/')
export function requestedAstContext(pathname: string): string | undefined {
  const prefix = '/ast/explorer/'
  if (!pathname.startsWith(prefix) || pathname.length === prefix.length) return undefined
  try { return decodeURIComponent(pathname.slice(prefix.length)) } catch { throw new Error('The AST context URI is invalid') }
}
export function importedAstHref(source: AstInstalledContext): string {
  return '/ast/explorer/' + encodeURIComponent(source.context) + '?' + new URLSearchParams({ library: 'imported', source: stringifyNativeJson({ id: source.id, version: source.version }) })
}
export function requestedImportedAstSource(search: string): AstImportedSource | null {
  const params = new URLSearchParams(search), libraries = params.getAll('library')
  if (libraries.length > 1 || (libraries.length === 1 && libraries[0] !== 'imported')) throw new Error('The imported AST library is invalid')
  if (params.has('selection')) throw new Error('Imported AST readers do not accept a PROJECT selection')
  if (!params.has('source')) return null
  if (params.getAll('source').length !== 1) throw new Error('The imported AST source is ambiguous')
  const source = parseNativeJson<unknown>(params.get('source')!)
  if (!source || typeof source !== 'object' || Array.isArray(source)) throw new Error('The imported AST source is invalid')
  const value = source as Record<string, unknown>
  if (Object.keys(value).some(key => key !== 'id' && key !== 'version') || typeof value.id !== 'string' || typeof value.version !== 'string' || !value.id || !value.version) throw new Error('The imported AST source is invalid')
  return { id: value.id, version: value.version }
}
export const importedAstRoute = (search: string) => new URLSearchParams(search).get('library') === 'imported'
export function projectReadModule(pathname: string, search: string): ProjectReadModule | null {
  let astContext: string | undefined
  try { astContext = requestedAstContext(pathname) } catch { astContext = undefined }
  if ((pathname === '/ast/contexts' || pathname === '/ast/explorer' || astContext === 'project') && !importedAstRoute(search)) return 'ast'
  if (fileHubRoute(pathname)) return 'hub'
  if (pathname === '/workspace') return 'now'
  if (pathname === '/task' || pathname.startsWith('/task/')) return 'task'
  if ((pathname === '/memory/explorer' || pathname.startsWith('/memory/explorer/')) && !userMemoryRoute(pathname)) return 'memory'
  if ((pathname === '/knowledge/explorer' || pathname.startsWith('/knowledge/explorer/')) && !importedKnowledgeRoute(search)) return 'knowledge'
  return null
}

export function importedKnowledgeHref(source: ImportedKnowledgeSource | null, slug?: string): string {
  const params = new URLSearchParams({ library: 'imported' })
  if (source) params.set('source', stringifyNativeJson(source))
  if (slug !== undefined) params.set('page', slug)
  return '/knowledge/explorer?' + params
}

export function requestedImportedKnowledgeSource(search: string): ImportedKnowledgeSource | null {
  const params = new URLSearchParams(search), libraries = params.getAll('library')
  if (libraries.length > 1 || (libraries.length === 1 && libraries[0] !== 'imported' && libraries[0] !== 'project')) throw new Error('The Knowledge library is invalid')
  if (!params.has('source')) return null
  if (libraries[0] === 'project' || params.has('selection')) throw new Error('The imported Knowledge selection is invalid')
  if (params.getAll('source').length !== 1) throw new Error('The imported Knowledge selection is ambiguous')
  const source = parseNativeJson<unknown>(params.get('source')!)
  if (!source || typeof source !== 'object' || Array.isArray(source)) throw new Error('The imported Knowledge selection is invalid')
  const fields = source as Record<string, unknown>
  if (fields.kind !== 'imported_knowledge' || Object.keys(fields).some(key => !['kind', 'artifact'].includes(key)) || !fields.artifact || typeof fields.artifact !== 'object' || Array.isArray(fields.artifact)) throw new Error('The imported Knowledge selection is invalid')
  const artifact = fields.artifact as Record<string, unknown>
  if (Object.keys(artifact).some(key => !['id', 'version', 'context_alias'].includes(key)) || typeof artifact.id !== 'string' || typeof artifact.version !== 'string' || (Object.prototype.hasOwnProperty.call(artifact, 'context_alias') && typeof artifact.context_alias !== 'string')) throw new Error('The imported Knowledge selection is invalid')
  return source as ImportedKnowledgeSource
}

export function projectRecordHref(kind: ProjectRecordKind, id: string, selection: ProjectLocator): string {
  const path = kind === 'task' ? '/task/explorer/' + encodeURIComponent(id)
    : kind === 'session' ? '/task/sessions/' + encodeURIComponent(id)
    : kind === 'memory' ? '/memory/explorer/project/' + encodeURIComponent(id)
    : '/knowledge/explorer'
  const params = new URLSearchParams({ selection: encodeProjectLocator(selection) })
  if (kind === 'knowledge') params.set('page', id)
  return path + '?' + params.toString()
}

export function userMemoryHref(id: string, observedScope: string): string {
  return '/memory/explorer/user/' + encodeURIComponent(id) + '?' + new URLSearchParams({ scope_id: observedScope }).toString()
}

export function requestedRecordSelection(search: string): ProjectLocator | null {
  const params = new URLSearchParams(search)
  if (!params.has('selection')) return null
  if (params.getAll('selection').length !== 1) throw new Error('The record selection is ambiguous')
  const value = parseNativeJson<unknown>(params.get('selection')!)
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('The record selection is invalid')
  const fields = value as Record<string, unknown>
  if (Object.keys(fields).some(key => !['project_id', 'project_version'].includes(key)) ||
    typeof fields.project_id !== 'string' || !fields.project_id || fields.project_id.includes('\0') ||
    typeof fields.project_version !== 'string' || !fields.project_version || fields.project_version.includes('\0')) throw new Error('The record selection is invalid')
  // URI encoding rejects lone UTF-16 surrogates without normalizing the locator.
  try { encodeURIComponent(fields.project_id); encodeURIComponent(fields.project_version) }
  catch { throw new Error('The record selection is invalid') }
  return { project_id: fields.project_id, project_version: fields.project_version }
}
