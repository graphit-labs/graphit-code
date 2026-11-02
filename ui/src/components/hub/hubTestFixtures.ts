import { useTaskWorkspace } from '@/components/task/taskWorkspaceState'
import type { TaskContext } from '@/api/task'
import { useAppStore } from '@/store/appStore'
import { mutationArtifact, type ArtifactPage, type FileOutcome, type InstalledFile, type InstalledFilePage, type MutationSelection } from '@/api/hubNative'

export const projectA = '01KSH1CRFFG8Z74B5ZS78WW808'
export const projectB = '01D39ZY06FGSCTVN4T2V9PKHFZ'
export function selectHubProject(id = projectA, version = 'feature/a', dir = '/a') {
  const selection: TaskContext = { project_dir: dir, project_id: id, project_version: version }
  useTaskWorkspace.setState({ module: 'hub', phase: 'ready', contexts: [selection], selection, error: '', refreshing: false })
  return selection
}
export function hubContext() {
  selectHubProject()
  useAppStore.setState({ projectsLoaded: true, activeProjectKey: 'workspace:a:/a', activeProjectOrigin: 'workspace', activeProjectId: projectA, activeProjectDir: '/a', activeProjectVersion: 'feature/a', activeAgent: 'kiro-v3', webIdentity: { state: 'authenticated', provider: 'corporate', subject: 'first-user' } })
}
export const catalog: ArtifactPage = { status: 'ok', producer: projectB, next_cursor: null, artifacts: [{ artifact_type: 'skill', id: 'vendor/reviewer', name: 'Engineering reviewer', description: '**Reusable context**\n\n- Uses `contracts`\n- [Read guide](https://example.test/guide)', tags: ['rust'], author: 'Fixture author', latest: null, versions: ['release-1','release-2'] }] }
export const installed: InstalledFile = { artifact_type: 'skill', id: 'vendor/reviewer', version: 'release-1', producer: projectB, target: { kind: 'shared_adapter', adapter: 'kiro' }, source_origin: 'HubCache', origin: { transport: 'broker', provider: 'corporate' }, alias: 'reviewer', entry_name: null, display: { name: 'Installed reviewer', description: '**Installed** guidance.', tags: ['rust'] } }
export const installedPage = (entries: InstalledFile[] = [installed]): InstalledFilePage => ({ status: 'ok', scope: { kind: 'project', project_id: projectA, project_version: 'feature/a' }, entries, next_offset: null })
export function completed(operation: FileOutcome['operation'], request: MutationSelection): FileOutcome {
  const artifact = mutationArtifact(request)
  if ('source' in request) return { status: 'ok', operation: 'install_local', scope: request.scope, cache: 'not_attempted', publication: 'Physical(Created)', source: 'local', producer: request.source.project_id, artifact_type: request.source.artifact_type, id: request.source.id, version: request.source.version, project: request.scope.project_id, project_version: request.scope.project_version, binding: request.binding, possible_parents: [] }
  return { status: 'ok', operation, scope: request.scope, artifact_type: artifact.artifact_type, id: artifact.id, version: artifact.version, cache: operation === 'install' || operation === 'update' ? 'Unchanged' : 'not_attempted', publication: operation === 'install' || operation === 'update' ? 'VerifiedUnchanged' : { change: operation === 'recover' ? 'Recovered' : operation === 'forget' ? 'Forgotten' : 'MetadataOnly', cleanup_pending: false } }
}
