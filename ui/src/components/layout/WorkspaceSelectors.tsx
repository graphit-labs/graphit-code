import { RefreshCw } from 'lucide-react';
import { ContextSelect } from './ContextSelect';
import { useAppStore } from '@/store/appStore';
import { workspaceTarget } from '@/store/appStore';
import { useWorkspaceRefresh } from './WorkspaceRefresh';
import { VersionSelect } from './VersionSelect';
import { useState } from 'react';

function agentLabel(id: string): string {
  const names: Record<string, string> = {
    vscode: 'VS Code', 'claude-code': 'Claude Code', antigravity: 'Antigravity',
    opencode: 'OpenCode', 'cursor-agent': 'Cursor Agent', qwen: 'Qwen Code',
    kimi: 'Kimi Code', deepcode: 'Deep Code',
  };
  return names[id] || id.charAt(0).toUpperCase() + id.slice(1);
}



/** The only global project/version/agent controls. Mounted once by AppShell. */
export function WorkspaceSelectors() {
  const { projects, projectTargets, activeProjectKey, activeProjectDir, projectName, activeAgent, supportedAgents,
    projectsLoaded, switchProject, switchProjectTarget, setActiveAgent } = useAppStore();
  const refresh = useWorkspaceRefresh();
  const [refreshTick, setRefreshTick] = useState(0);
  const targets = projectTargets.length ? projectTargets : projects.map(workspaceTarget);
  const value = activeProjectKey || targets.find(target => target.dir === activeProjectDir)?.key || '';
  const active = targets.find(target => target.key === value);
  const projectDisplay = !projectsLoaded ? 'Loading projects…' : active
    ? `${active.name}${active.origin === 'hub' ? ' · Hub' : ''}${active.availability === 'stale' ? ' (unavailable)' : ''}`
    : activeProjectDir ? `${projectName || activeProjectDir} (unavailable)` : targets.length ? 'Select project' : 'No projects available';
  const agentDisplay = !projectsLoaded ? 'Loading…' : supportedAgents.includes(activeAgent) ? agentLabel(activeAgent)
    : activeAgent ? `${agentLabel(activeAgent)} (unavailable)` : supportedAgents.length ? 'Select agent' : 'No agents available';
  return <div className="workspace-selectors" role="group" aria-label="Working context">
    <ContextSelect label="Project" value={value} display={projectDisplay}
      groups={[['workspace', 'Workspace'], ['hub', 'Hub · remote']]}
      options={targets.map(target => ({ value: target.key, label: target.name, group: target.origin,
        detail: target.origin === 'workspace' ? target.dir : `Remote project · ${target.id}` }))}
      disabled={!projectsLoaded || !targets.length} onChange={(key) => {
        const target = targets.find(candidate => candidate.key === key);
        if (target?.origin === 'workspace' && !projectTargets.length) switchProject(target.dir || '');
        else switchProjectTarget(key);
      }} />
    <VersionSelect refreshTick={refreshTick} />
    <ContextSelect label="Agent" value={activeAgent} display={agentDisplay}
      groups={[['agent', 'Development agent']]}
      options={supportedAgents.map(agent => ({ value: agent, label: agentLabel(agent), group: 'agent' }))}
      disabled={!projectsLoaded || !supportedAgents.length} onChange={setActiveAgent} />
    <button className="context-refresh" type="button" onClick={() => { setRefreshTick(tick => tick + 1); void refresh?.refresh(); }}
      disabled={!refresh || refresh.refreshing} aria-label="Refresh" title="Refresh page and working context" aria-busy={refresh?.refreshing || false}>
      <RefreshCw size={17} className={refresh?.refreshing ? 'animate-spin' : ''} aria-hidden="true" />
    </button>
    {refresh?.error && <div className="workspace-refresh-error" role="alert">{refresh.error}</div>}
  </div>;
}
