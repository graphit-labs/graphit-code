import * as Select from '@radix-ui/react-select';
import { Check, ChevronDown, ChevronUp, RefreshCw } from 'lucide-react';
import { useAppStore } from '@/store/appStore';
import { workspaceTarget, type ProjectOrigin } from '@/store/appStore';
import { useWorkspaceRefresh } from './WorkspaceRefresh';

function agentLabel(id: string): string {
  const names: Record<string, string> = {
    vscode: 'VS Code', 'claude-code': 'Claude Code', 'gemini-code': 'Gemini Code',
    opencode: 'OpenCode', 'cursor-agent': 'Cursor Agent', qwen: 'Qwen Code',
    kimi: 'Kimi Code', deepcode: 'Deep Code',
  };
  return names[id] || id.charAt(0).toUpperCase() + id.slice(1);
}

function ContextSelect({ label, value, display, options, disabled, onChange }: {
  label: string; value: string; display: string; disabled: boolean;
  options: { value: string; label: string; detail?: string; group?: ProjectOrigin }[]; onChange: (value: string) => void;
}) {
  const groups = label === 'Project'
    ? ([['workspace', 'Workspace'], ['hub', 'Hub · remote']] as const)
    : ([['workspace', label === 'Project' ? 'Workspace' : 'Development agent']] as const);
  return <Select.Root value={value} onValueChange={onChange} disabled={disabled}>
    <Select.Trigger className="context-select" aria-label={label} title={options.find(o => o.value === value)?.detail || display}>
      <span className="context-select-label">{label}</span>
      <span className="context-select-value"><Select.Value placeholder={display}>{display}</Select.Value></span>
      <Select.Icon><ChevronDown size={13} /></Select.Icon>
    </Select.Trigger>
    <Select.Portal>
      <Select.Content className="context-select-menu" position="popper" sideOffset={6} align="end" collisionPadding={12}>
        <Select.ScrollUpButton className="context-select-scroll"><ChevronUp size={14} /></Select.ScrollUpButton>
        <Select.Viewport className="context-select-options">
          {groups.map(([group, groupLabel]) => {
            const groupOptions = options.filter(option => label !== 'Project' || option.group === group);
            if (!groupOptions.length) return null;
            return <Select.Group key={group}>
              <Select.Label className="context-select-menu-label">{groupLabel}</Select.Label>
              {groupOptions.map(option => <Select.Item className="context-select-option" key={option.value} value={option.value} textValue={option.label}>
                <span className="context-select-option-copy"><Select.ItemText>{option.label}</Select.ItemText>{option.detail && <small>{option.detail}</small>}</span>
                <Select.ItemIndicator className="context-select-check"><Check size={15} /></Select.ItemIndicator>
              </Select.Item>)}
            </Select.Group>;
          })}
        </Select.Viewport>
        <Select.ScrollDownButton className="context-select-scroll"><ChevronDown size={14} /></Select.ScrollDownButton>
      </Select.Content>
    </Select.Portal>
  </Select.Root>;
}

/** The only global project/agent controls. Mounted once by AppShell. */
export function WorkspaceSelectors() {
  const { projects, projectTargets, activeProjectKey, activeProjectDir, projectName, activeAgent, supportedAgents,
    projectsLoaded, switchProject, switchProjectTarget, setActiveAgent } = useAppStore();
  const refresh = useWorkspaceRefresh();
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
      options={targets.map(target => ({ value: target.key, label: target.name, group: target.origin,
        detail: target.origin === 'workspace' ? target.dir : `Remote project · ${target.id}` }))}
      disabled={!projectsLoaded || !targets.length} onChange={(key) => {
        const target = targets.find(candidate => candidate.key === key);
        if (target?.origin === 'workspace' && !projectTargets.length) switchProject(target.dir || '');
        else switchProjectTarget(key);
      }} />
    <ContextSelect label="Agent" value={activeAgent} display={agentDisplay}
      options={supportedAgents.map(agent => ({ value: agent, label: agentLabel(agent) }))}
      disabled={!projectsLoaded || !supportedAgents.length} onChange={setActiveAgent} />
    <button className="context-refresh" type="button" onClick={() => void refresh?.refresh()}
      disabled={!refresh || refresh.refreshing} aria-label="Refresh" title="Refresh page and working context" aria-busy={refresh?.refreshing || false}>
      <RefreshCw size={17} className={refresh?.refreshing ? 'animate-spin' : ''} aria-hidden="true" />
    </button>
    {refresh?.error && <div className="workspace-refresh-error" role="alert">{refresh.error}</div>}
  </div>;
}
