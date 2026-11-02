import { useMemo } from 'react'
import { nativeBindings, type HubScope, type NativeBinding } from '@/api/hubNative'
import { nativeSelectionKey } from '@/api/nativeRecordNavigation'
import { useAppStore } from '@/store/appStore'
import { useTaskWorkspace } from '@/components/task/taskWorkspaceState'

export function useHubContext() {
  const { apiBase, webIdentity, activeAgent } = useAppStore()
  const workspace = useTaskWorkspace()
  const identityKey = JSON.stringify([apiBase, webIdentity])
  const available = workspace.module === 'hub' && workspace.phase === 'ready'
  const selection = available ? workspace.selection : null
  const selectionKey = JSON.stringify([identityKey, nativeSelectionKey(selection), activeAgent])
  const binding = nativeBindings.includes(activeAgent as NativeBinding) ? activeAgent as NativeBinding : null
  const project = useMemo<HubScope | null>(() => selection ? { kind: 'project', project_id: selection.project_id, project_version: selection.project_version } : null, [selection])
  return { identity: webIdentity, identityKey, binding, project, selectionKey, key: selectionKey,
    versionLoading: workspace.module !== 'hub' || workspace.phase === 'loading',
    projectUnavailable: !available ? 'Wait for authorized Hub working contexts in the header.' : !selection ? 'Choose an authorized project and data version in the header, or use the installation cache.' : !binding ? 'Choose a supported native binding in the header.' : '',
  }
}
export type HubContext = ReturnType<typeof useHubContext>
export function scopeLabel(scope: HubScope) { return scope.kind === 'global_cache' ? 'Installation cache' : scope.project_id + ' @ ' + scope.project_version }
export const bindingLabel = (binding: string) => ({ 'kiro-v3': 'Kiro V3', opencode: 'OpenCode 2', antigravity: 'Antigravity', codex: 'Codex', claude: 'Claude', cursor: 'Cursor', qwen: 'Qwen', kimi: 'Kimi' }[binding] || binding)
