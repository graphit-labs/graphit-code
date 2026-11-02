import { AgentExecution, appendProgress, type ExecutionOutcome } from "@/components/shared/AgentExecution";
import type { AgentProgress } from "@/api/agentStream";
import { useCallback, useState, useRef, useEffect } from "react";
import { nativeAstApi, astSelection, type AstImportedSource } from '@/api/nativeAst';
import { nativeSelectionKey } from '@/api/nativeRecordNavigation';
import type { TaskContext } from '@/api/task';
import { invalidateTaskWorkspace } from '@/components/task/taskWorkspaceState';
import { astApi } from "@/api/ast";
import { useAppStore } from "@/store/appStore";
import { agentFeaturesEnabled } from "@/lib/utils";
import { WorkTabs, WorkNotice } from "@/components/shared/EngineeringUI";

interface QueryBarProps {
  contextId?: string;
  selection?: TaskContext | null;
  nativeProject?: boolean;
  importedSource?: AstImportedSource | null;
  projectDir?: string;
  projectId?: string;
  onQueryResult: (result: unknown, executedQuery: string) => void;
  onQueryAbort?: (controller: AbortController) => void;
  onQueryStart?: (query: string) => () => boolean;
  onQueryError?: (error: string) => void;
  loading: boolean;
  setLoading: (value: boolean) => void;
  collapsed?: boolean;
  onCollapsedClick?: () => void;
}
const EXAMPLES = [
  "MATCH (n) RETURN n LIMIT 50",
  "MATCH (n:Function) RETURN n LIMIT 30",
  "MATCH (caller:Function)-[:CALLS]->(callee:Function) WHERE caller.name = 'handleRequest' RETURN DISTINCT callee.uid AS uid, callee.name AS name, callee.path AS path LIMIT 40",
  "MATCH (n:File) RETURN n LIMIT 20",
];
export function QueryBar({
  contextId,
  selection,
  nativeProject,
  importedSource,
  projectDir,
  projectId,
  onQueryResult,
  onQueryStart,
  onQueryAbort,
  onQueryError,
  loading,
  setLoading,
}: QueryBarProps) {
  const isNativeProject = nativeProject ?? (!contextId || contextId === "project");
  const activeAgent = useAppStore(state => state.activeAgent);
  const [mode, setMode] = useState("cypher"),
    [query, setQuery] = useState(""),
    [prompt, setPrompt] = useState("");
  const [generating, setGenerating] = useState(false),
    [error, setError] = useState(""),
    [generated, setGenerated] = useState(false);
  const generation = useRef(0);
  const abort = useRef<AbortController | null>(null);
  const [progress, setProgress] = useState<AgentProgress[]>([]);
  const [outcome, setOutcome] = useState<ExecutionOutcome>("idle");
  const scope = JSON.stringify([nativeSelectionKey(selection ?? null), projectDir, projectId, contextId, activeAgent, isNativeProject, importedSource]);
  const [executionScope, setExecutionScope] = useState(scope);
  if (executionScope !== scope) {
    setExecutionScope(scope);
    setGenerating(false); setProgress([]); setOutcome("idle"); setError("");
  }
  const abortScope = useCallback(() => { generation.current++; abort.current?.abort(); }, []);
  useEffect(() => {
    let active = true;
    // The loading indicator belongs to the parent, outside this component's state.
    queueMicrotask(() => { if (active) setLoading(false); });
    return () => { active = false; abortScope(); };
  }, [scope, setLoading, abortScope]);
  const execute = async () => {
    if (!query.trim()) return;
    abort.current?.abort(); const controller = new AbortController(); abort.current = controller;
    const id = ++generation.current;
    const isCurrent = onQueryStart?.(query.trim()) ?? (() => true);
    onQueryAbort?.(controller);
    setLoading(true);
    setError("");
    try {
      if (isNativeProject && !selection) throw new Error('Select an AST working context before querying.');
      if (!isNativeProject && !importedSource) throw new Error('Select an installed AST context before querying.');
      const result = isNativeProject
        ? await nativeAstApi.query(astSelection(selection!), query.trim(), {}, controller.signal)
        : await nativeAstApi.importedQuery(importedSource!, query.trim(), {}, controller.signal);
      if (id === generation.current && !controller.signal.aborted && isCurrent()) onQueryResult(result, query.trim());
    } catch (e) {
      if (id === generation.current && !controller.signal.aborted && isCurrent()) { if (isNativeProject) invalidateTaskWorkspace(e, 'ast'); if (onQueryError) onQueryError((e as Error).message); else setError((e as Error).message); }
    } finally {
      if (id === generation.current && isCurrent()) setLoading(false);
    }
  };
  const generate = async () => {
    if (!prompt.trim()) return;
    if (!isNativeProject && !contextId) { setError("Select an imported AST context before drafting a query."); return; }
    if (selection && !projectDir) { setError("This source path cannot be represented by the active AI reader."); return; }
    const id = ++generation.current;
    abort.current?.abort(); const controller = new AbortController(); abort.current = controller;
    setProgress([]); setOutcome("idle");
    setGenerating(true);
    setError("");
    try {
      const result = await astApi.generateCypher(
        prompt.trim(),
        contextId,
        projectDir,
        { signal: controller.signal, onProgress: event => { if (id === generation.current && !controller.signal.aborted) setProgress(items => appendProgress(items, event)); } },
        projectId,
      );
      if (id !== generation.current) return;
      if (!result.cypher)
        throw new Error(
          "No query was generated. Refine the question and try again.",
        );
      setQuery(result.cypher);
      setMode("cypher");
      setGenerated(true); setOutcome("completed");
    } catch (e) {
      if (id === generation.current) { setError((e as Error).message); setOutcome("failed"); }
    } finally {
      if (id === generation.current) setGenerating(false);
    }
  };
  return (
    <div className="work-form">
      <AgentExecution events={progress} running={generating} outcome={outcome} onCancel={() => { abort.current?.abort(); generation.current++; setGenerating(false); setOutcome("cancelled"); setError("Generation stopped."); }} />
      <WorkTabs
        value={mode}
        onChange={setMode}
        items={
          agentFeaturesEnabled()
            ? [
                ["cypher", "Write Cypher"],
                ["nl", "Draft with AI"],
              ]
            : [["cypher", "Write Cypher"]]
        }
        label="Query authoring"
      />
      {generated && mode === "cypher" && (
        <WorkNotice title="Draft ready for review">
          Check the scope, relationship and limit before running. The query has
          not been executed.
        </WorkNotice>
      )}
      {error && (
        <WorkNotice tone="error" title="Query could not complete">
          {error}
        </WorkNotice>
      )}
      <label className="work-field">
        <span>
          {mode === "nl" ? "What do you want to understand?" : "Cypher query"}
        </span>
        <textarea
          rows={5}
          value={mode === "nl" ? prompt : query}
          onChange={(e) =>
            mode === "nl" ? setPrompt(e.target.value) : setQuery(e.target.value)
          }
          placeholder={
            mode === "nl"
              ? "Find callers of the authorization handler"
              : "MATCH (n:Function) RETURN n LIMIT 30"
          }
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              if (!loading && !generating)
                void (mode === "nl" ? generate() : execute());
            }
          }}
        />
      </label>
      <div className="work-actions">
        <button
          className="work-button primary"
          onClick={() => void (mode === "nl" ? generate() : execute())}
          disabled={
            loading || generating || !(mode === "nl" ? prompt : query).trim()
          }
        >
          {generating
            ? "Drafting…"
            : loading
              ? "Running…"
              : mode === "nl"
                ? "Generate draft"
                : "Run query"}
        </button>
        <small className="text-muted-foreground">
          ⌘ / Ctrl + Enter · read-only execution
        </small>
        {loading && <button className="work-button" onClick={() => { abort.current?.abort(); generation.current++; setLoading(false); setError("Query stopped."); }}>Cancel query</button>}
      </div>
      <details className="work-disclosure">
        <summary>Query starters</summary>
        <div className="work-form">
          {EXAMPLES.map((q) => (
            <button
              className="text-left text-xs font-mono text-primary"
              key={q}
              onClick={() => {
                setQuery(q);
                setMode("cypher");
                setGenerated(false);
              }}
            >
              {q}
            </button>
          ))}
        </div>
      </details>
    </div>
  );
}
export function QueryBarCollapsed({ onClick }: { onClick: () => void }) {
  return (
    <button className="work-button" onClick={onClick}>
      Open query editor
    </button>
  );
}
export function QueryBarCollapseButton({ onClick }: { onClick: () => void }) {
  return (
    <button className="work-button" onClick={onClick}>
      Close query editor
    </button>
  );
}
