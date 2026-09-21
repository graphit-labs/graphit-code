import * as Select from '@radix-ui/react-select';
import { Check, ChevronDown, ChevronUp, RefreshCw } from 'lucide-react';
import { useAppStore } from '@/store/appStore';
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
  options: { value: string; label: string; detail?: string }[]; onChange: (value: string) => void;
}) {
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
          <Select.Group>
            <Select.Label className="context-select-menu-label">{label === 'Project' ? 'Working project' : 'Development agent'}</Select.Label>
            {options.map(option => <Select.Item className="context-select-option" key={option.value} value={option.value} textValue={option.label}>
              <span className="context-select-option-copy"><Select.ItemText>{option.label}</Select.ItemText>{option.detail && <small>{option.detail}</small>}</span>
              <Select.ItemIndicator className="context-select-check"><Check size={15} /></Select.ItemIndicator>
            </Select.Item>)}
          </Select.Group>
        </Select.Viewport>
        <Select.ScrollDownButton className="context-select-scroll"><ChevronDown size={14} /></Select.ScrollDownButton>
      </Select.Content>
    </Select.Portal>
  </Select.Root>;
}

/** The only global project/agent controls. Mounted once by AppShell. */
export function WorkspaceSelectors() {
  const { projects, activeProjectDir, projectName, activeAgent, supportedAgents,
    projectsLoaded, switchProject, setActiveAgent } = useAppStore();
  const refresh = useWorkspaceRefresh();
  const active = projects.find(p => p.dir === activeProjectDir);
  const projectDisplay = !projectsLoaded ? 'Loading projects…' : active?.name || (activeProjectDir
    ? `${projectName || activeProjectDir} (unavailable)` : projects.length ? 'Select project' : 'No projects available');
  const agentDisplay = !projectsLoaded ? 'Loading…' : supportedAgents.includes(activeAgent) ? agentLabel(activeAgent)
    : activeAgent ? `${agentLabel(activeAgent)} (unavailable)` : supportedAgents.length ? 'Select agent' : 'No agents available';
  return <div className="workspace-selectors" role="group" aria-label="Working context">
    <ContextSelect label="Project" value={activeProjectDir} display={projectDisplay}
      options={projects.map(p => ({ value: p.dir, label: p.name, detail: p.dir }))}
      disabled={!projectsLoaded || !projects.length} onChange={switchProject} />
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
