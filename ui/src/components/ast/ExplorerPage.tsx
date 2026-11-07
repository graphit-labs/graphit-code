import { StyledSelect } from "@/components/shared/StyledSelect";
import { usePageRefresh, refreshAll } from "@/components/layout/WorkspaceRefresh";
import { useEffect, useLayoutEffect, useState, useRef, useCallback, useMemo } from "react";
import { useNavigate, useLocation } from "react-router-dom";
import {
  type GraphNode,
  type GraphEdge,
  type SchemaResponse,
} from "@/api/ast";
import { useAstImports } from "./useAstImports";
import { loadNativeNeighborhood, resolveNativeNode, type NeighborhoodLoader } from "./neighborhood";
import { RelationshipExplorer } from "./RelationshipExplorer";
import { NativeEntityDetails } from "./NativeEntityDetails";
import { QueryBar } from "./QueryBar";
import { CodePanel } from "./CodePanel";
import { TabularResults } from "./TabularResults";


import {
  WorkBadge,
  WorkPage,
  WorkHeader,
  WorkSection,
  WorkSearch,
  WorkTabs,
  WorkNotice,
  WorkEmpty,
  FactList,
  RecordLink,
} from "@/components/shared/EngineeringUI";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";

import { nativeAstApi, astSelection, type AstKey, type AstSourceReply, type AstSchema, type AstQuery, type AstSearchMode, type AstSearchMetric, type AstSearchReply } from '@/api/nativeAst';
import { encodeProjectLocator, nativeSelectionKey, projectReadModule, requestedImportedAstSource, requestedAstContext, importedAstHref } from '@/api/nativeRecordNavigation';
import { stringifyNativeJson, parseNativeJson, type NativeInteger } from '@/api/nativeJson';
import { useTaskWorkspace, invalidateTaskWorkspace } from '@/components/task/taskWorkspaceState';
import { TaskWorkspaceNotice } from '@/components/task/TaskWorkspace';
import { nativeSchemaView, nativeSampleView, nativeQueryView } from './nativeGraphView';

function PublishedFileObservations({ path, schema, read }: {
  path: string;
  schema: AstSchema;
  read: (path: string, signal: AbortSignal) => Promise<AstQuery>;
}) {
  const [open, setOpen] = useState(false);
  const [retry, setRetry] = useState(0);
  const [result, setResult] = useState<{ value?: Record<string, unknown>; error?: string } | null>(null);
  const details = useRef<HTMLDetailsElement>(null);
  const summary = useRef<HTMLElement>(null);
  const request = useRef<AbortController | null>(null);
  useEffect(() => {
    if (!open || !details.current?.open) return;
    const controller = new AbortController();
    request.current = controller;
    setResult(null);
    const load = async () => {
      try {
        const reply = await read(path, controller.signal);
        if (controller.signal.aborted || !details.current?.open) return;
        const observed = nativeQueryView(reply, schema).nodes;
        if (observed.length > 1) throw new Error('The published file observation is ambiguous.');
        const properties = observed[0]?.properties;
        const value = properties ? { ...properties, ...Object.fromEntries(['resolution_outcomes_json', 'diagnostics_json', 'capabilities_json'].filter(key => typeof properties[key] === 'string').map(key => [key, parseNativeJson(properties[key] as string)])) } : undefined;
        setResult({ value });
      } catch (cause) {
        if (!controller.signal.aborted && details.current?.open) setResult({ error: cause instanceof Error ? cause.message : 'Published file observations could not load.' });
      }
    };
    void load();
    return () => controller.abort();
  }, [open, retry, path, schema, read]);
  return <details ref={details} className="work-disclosure" open={open} onToggle={event => {
    const expanded = event.currentTarget.open;
    if (!expanded) { request.current?.abort(); setResult(null); }
    setOpen(expanded);
  }}>
    <summary ref={summary}>Published file observations</summary>
    {open && <div>
      <p>These are the indexed resolution outcomes and evidence. Missing edges do not establish the absence of a relationship.</p>
      {!result ? <>
        <LoadingSpinner label="Loading published file observations…" />
        <button className="work-button" onClick={() => { request.current?.abort(); setOpen(false); setResult(null); summary.current?.focus(); }}>Cancel observations</button>
      </> : result.error !== undefined ? <WorkNotice tone="error" title="Published observations unavailable">
        {result.error}
        <button className="work-button" onClick={() => setRetry(value => value + 1)}>Retry observations</button>
      </WorkNotice> : result.value ? <pre className="work-code">{stringifyNativeJson(result.value, 2)}</pre> : <p>No published file observations for this source.</p>}
    </div>}
  </details>;
}

export default function ExplorerPage() {

  const location = useLocation();
  const contextURI = useMemo(() => {
    try { return { context: requestedAstContext(location.pathname), error: '' }; }
    catch (cause) { return { context: undefined, error: cause instanceof Error ? cause.message : 'The AST context URI is invalid.' }; }
  }, [location.pathname]);
  const context = contextURI.context;
  const nativeProject = projectReadModule(location.pathname, location.search) === "ast";
  const importedBinding = useMemo(() => {
    if (nativeProject) return { source: null, error: '' };
    try { return { source: requestedImportedAstSource(location.search), error: '' }; }
    catch (cause) { return { source: null, error: cause instanceof Error ? cause.message : 'The imported AST source is invalid.' }; }
  }, [nativeProject, location.search]);
  const imports = useAstImports(!nativeProject && !contextURI.error && !importedBinding.error);
  const matches = imports.items.filter(item => item.context === context && (!importedBinding.source || item.id === importedBinding.source.id && item.version === importedBinding.source.version));
  const importedSource = matches.length === 1 ? matches[0] : null;
  const scopeError = importedBinding.error || (!nativeProject && context === 'project' ? 'A PROJECT graph cannot be selected as an imported index.' : '') || (!nativeProject && context && !imports.loading && !imports.error && !importedSource ? 'The exact imported AST context was not returned by the installed catalogue.' : '');
  const workspace = useTaskWorkspace();
  const selection = nativeProject && workspace.module === 'ast' && workspace.phase === 'ready' ? workspace.selection : null;
  const physicalBinding = selection;
  const projectKey = nativeProject ? nativeSelectionKey(selection) : stringifyNativeJson(importedSource && [importedSource.id, importedSource.version]);
  const projectId = nativeProject ? selection?.project_id : undefined;
  const projectDir = nativeProject ? selection?.project_dir : undefined;
  const entityScope = nativeProject ? selection && `${selection.project_id} / ${selection.project_version}` : importedSource && `${importedSource.id}@${importedSource.version}`;
  const navigate = useNavigate();
  const [filePath, setFilePath] = useState('');
  const [fileRead, setFileRead] = useState<string | null>(null);
  const [searchMode, setSearchMode] = useState<AstSearchMode>('semantic');
  const [searchMetric, setSearchMetric] = useState<AstSearchMetric>('cosine');
  const [rerank, setRerank] = useState<'configuration' | 'off' | 'on'>('configuration');
  const [searchReply, setSearchReply] = useState<AstSearchReply | null>(null);
  const [view, setView] = useState("investigate");
  const [query, setQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState("");
  const [selected, setSelected] = useState<GraphNode | null>(null);
  const [source, setSource] = useState<{
    path: string;
    content: string;
    line?: NativeInteger;
    provenance: string;
    native: AstSourceReply;
    request: number;
    entity?: { key: AstKey; kind: string };
    observationSchema?: AstSchema;
  } | null>(null);
  const [sourceLoading, setSourceLoading] = useState(false);
  const [sourceError, setSourceError] = useState("");
  const [sourceStopped, setSourceStopped] = useState(false);
  const inspection = useRef<HTMLElement>(null);
  useEffect(() => {
    if (selected && inspection.current) {
      inspection.current.focus({ preventScroll: true });
      inspection.current.scrollIntoView?.({ block: "start", behavior: "smooth" });
    }
  }, [selected]);
  const [relation, setRelation] = useState("source");
  const [edgeType, setEdgeType] = useState("CALLS");
  const [targetType, setTargetType] = useState("");
  const [related, setRelated] = useState<GraphNode[]>([]);
  const [relationLoading, setRelationLoading] = useState(false);
  const [schema, setSchema] = useState<SchemaResponse>({
    nodes: [],
    edges: [],
    node_labels: [],
    edge_types: [],
    langs: [],
    backend: "",
  });
  const nativeSchema = useRef<AstSchema | null>(null);
  const schemaAbort = useRef<AbortController | null>(null);
  const graphAbort = useRef<AbortController | null>(null), relationAbort = useRef<AbortController | null>(null), sourceAbort = useRef<AbortController | null>(null);
  const [nodes, setNodes] = useState<GraphNode[]>([]);
  const [links, setLinks] = useState<GraphEdge[]>([]);
  const [tabular, setTabular] = useState<{
    columns: string[];
    rows: unknown[][];
  } | null>(null);
  const [queryLoading, setQueryLoading] = useState(false);
  const [refreshingGraph, setRefreshingGraph] = useState(false);
  const [resultOrigin, setResultOrigin] = useState("");
  const [resultKind, setResultKind] = useState<"sample" | "search" | "query">("sample");
  const [resultVersion, setResultVersion] = useState(0);
  const sourceRequest = useRef(0),
    relationRequest = useRef(0),
    graphRequest = useRef(0);
  const lastSearch = useRef("");
  const initialSample = useRef("");
  const lastGraphQuery = useRef<{ cypher_query?: string } | null>(null);
  const scope = useRef("");
  const currentScope = stringifyNativeJson([nativeProject, projectKey, context, nativeProject ? workspace.phase : importedSource?.context]);
  useLayoutEffect(() => { scope.current = currentScope; nativeSchema.current = null; }, [currentScope]);
  const [dataScope, setDataScope] = useState(currentScope);
  if (dataScope !== currentScope) {
    setDataScope(currentScope);
    setSelected(null);
    setSource(null);
    setSourceError("");
    setRelated([]);
    setNodes([]);
    setLinks([]);
    setTabular(null);
    setResultOrigin("");
    setResultKind("sample");
    setQueryLoading(false);
    setRefreshingGraph(false);
    setQuery("");
    setError("");
    setSearching(false);
    setSourceLoading(false); setSourceStopped(false);
    setRelationLoading(false);
    setSchema({
      nodes: [],
      edges: [],
      node_labels: [],
      edge_types: [],
      langs: [],
      backend: "",
    });
    setSearchReply(null); setFileRead(null); setFilePath('');
  }
  const cancelReaders = useCallback(() => {
    sourceRequest.current++; relationRequest.current++; graphRequest.current++;
    graphAbort.current?.abort(); relationAbort.current?.abort(); sourceAbort.current?.abort();
  }, []);
  const readSchema = useCallback(async (signal?: AbortSignal) => {
    if (scopeError) throw new Error(scopeError);
    if (nativeProject && !selection || !nativeProject && !importedSource) throw new Error('Select an admitted AST context.');
    const binding = currentScope;
    const raw = nativeProject ? await nativeAstApi.schema(astSelection(selection!), signal) : await nativeAstApi.importedSchema(importedSource!, signal);
    signal?.throwIfAborted();
    if (scope.current !== binding) throw new DOMException('The AST context changed.', 'AbortError');
    nativeSchema.current = raw;
    return nativeSchemaView(raw);
  }, [nativeProject, selection, importedSource, currentScope, scopeError]);
  const readGraph = useCallback(async (cypher?: string, signal?: AbortSignal) => {
    if (scopeError) throw new Error(scopeError);
    if (!nativeSchema.current || nativeProject && !selection || !nativeProject && !importedSource) throw new Error('Load an admitted AST context and schema first.');
    const observed = nativeSchema.current;
    if (cypher !== undefined) return nativeQueryView(nativeProject ? await nativeAstApi.query(astSelection(selection!), cypher, {}, signal) : await nativeAstApi.importedQuery(importedSource!, cypher, {}, signal), observed);
    return nativeSampleView(nativeProject ? await nativeAstApi.sample(astSelection(selection!), signal) : await nativeAstApi.importedSample(importedSource!, signal), observed);
  }, [scopeError, nativeProject, selection, importedSource]);
  useEffect(() => {
    cancelReaders();
    lastSearch.current = ''; lastGraphQuery.current = null; initialSample.current = '';
    if ((nativeProject && !selection) || (!nativeProject && !importedSource) || scopeError) return;
    schemaAbort.current?.abort(); const controller = new AbortController(); schemaAbort.current = controller;
    const startedScope = currentScope;
    void readSchema(controller.signal).then(value => {
      if (!controller.signal.aborted && scope.current === startedScope) setSchema(value);
    }).catch(cause => {
      if (!controller.signal.aborted && scope.current === startedScope) { if (nativeProject) invalidateTaskWorkspace(cause, 'ast'); setError(cause.message); }
    });
    return () => { controller.abort(); schemaAbort.current?.abort(); cancelReaders(); };
  }, [currentScope, nativeProject, selection, readSchema, context, importedSource, scopeError, cancelReaders]);
  const exploreNeighborhood: NeighborhoodLoader = useCallback((node, previous, signal) => {
    if (!nativeSchema.current || nativeProject && !selection || !nativeProject && !importedSource) return Promise.reject(new Error('Load an admitted AST context and schema first.'));
    return loadNativeNeighborhood(node, nativeSchema.current, nativeProject ? astSelection(selection!) : importedSource!, previous, signal);
  }, [nativeProject, selection, importedSource]);
  const beginResult = useCallback((origin: string, kind: "sample" | "search" | "query" = "sample") => {
    graphAbort.current?.abort(); relationAbort.current?.abort(); sourceAbort.current?.abort();
    setResultKind(kind);
    setRefreshingGraph(false);
    setView("map");
    setResultOrigin(origin);
    setResultVersion(version => version + 1);
    setNodes([]); setLinks([]); setTabular(null); setSearchReply(null);
    setSelected(null); setSource(null); setSourceError(""); setRelated([]); setFileRead(null);
    sourceRequest.current++; relationRequest.current++;
    setSourceLoading(false); setRelationLoading(false);
    setError("");
  }, [setFileRead, setSearchReply, setResultKind, setRefreshingGraph, setView, setResultOrigin, setResultVersion, setNodes, setLinks, setTabular, setSelected, setSource, setSourceError, setRelated, setSourceLoading, setRelationLoading, setError]);
  const clearSearch = () => {
    graphAbort.current?.abort(); graphRequest.current++;
    setSearching(false); setSearchReply(null); setNodes([]); setLinks([]); setError('');
    lastSearch.current = '';
  };
  const search = async (text = query, present = true) => {
    if (!text.trim()) return;
    lastSearch.current = text; lastGraphQuery.current = null;
    const id = ++graphRequest.current, startedScope = currentScope;
    if (present) beginResult('Search results · ' + text, 'search');
    graphAbort.current?.abort(); const controller = new AbortController(); graphAbort.current = controller;
    setQueryLoading(false); setSearching(true); setError(''); setSearchReply(null);
    try {
      if (nativeProject && !selection || !nativeProject && !importedSource) throw new Error('Select an admitted AST context.');
      const input = { query: text, mode: searchMode, k: 20, metric: searchMetric, ...(rerank === 'configuration' ? {} : { rerank: rerank === 'on' }) };
      const reply = nativeProject ? await nativeAstApi.search(astSelection(selection!), input, controller.signal) : await nativeAstApi.importedSearch(importedSource!, input, controller.signal);
      if (controller.signal.aborted || id !== graphRequest.current || scope.current !== startedScope) return;
      setSearchReply(reply);
      setNodes(reply.status === 'available' ? reply.results.map((hit, index) => ({
        id: stringifyNativeJson([startedScope, hit.entity_key, hit.uid, hit.data_key]),
        name: hit.name, label: hit.kind, type: hit.kind, file: hit.path, line: hit.start_line ?? undefined,
        properties: { ...hit, lang: hit.language, search_rank: index + 1 },
      })) : []);
      setLinks([]); setTabular(null);
    } catch (cause) {
      if (!controller.signal.aborted && id === graphRequest.current && scope.current === startedScope) { if (nativeProject) invalidateTaskWorkspace(cause, 'ast'); setError(cause instanceof Error ? cause.message : 'The AST search failed.'); }
    } finally { if (!controller.signal.aborted && id === graphRequest.current && scope.current === startedScope) setSearching(false); }
  };
  const cancelSource = useCallback(() => {
    sourceRequest.current++; sourceAbort.current?.abort(); setSourceLoading(false); setSourceStopped(true);
  }, [setSourceLoading, setSourceStopped]);
  const readFileObservations = useCallback((path: string, signal: AbortSignal) => {
    const query = 'MATCH (f:AstFileObservation) WHERE f.path = $source_path RETURN f LIMIT 2';
    if (nativeProject) {
      if (!selection) return Promise.reject(new Error('Select an admitted AST context.'));
      return nativeAstApi.query(astSelection(selection), query, { source_path: path }, signal);
    }
    if (!importedSource) return Promise.reject(new Error('Select an admitted AST context.'));
    return nativeAstApi.importedQuery(importedSource, query, { source_path: path }, signal);
  }, [nativeProject, selection, importedSource]);
  const openFile = useCallback(
    async (path: string, line?: NativeInteger, entity?: { key: AstKey; kind: string }) => {
      sourceAbort.current?.abort();
      const controller = new AbortController(); sourceAbort.current = controller;
      const id = ++sourceRequest.current, startedScope = currentScope;
      const isCurrent = () => !controller.signal.aborted && id === sourceRequest.current && scope.current === startedScope;
      setSource(previous => previous?.path === path ? previous : null); setSourceLoading(true); setSourceStopped(false); setSourceError(""); setError("");
      try {
        if (nativeProject && !selection || !nativeProject && !importedSource) throw new Error('Select an admitted AST context.');
        const input = { path, ...(entity ? { entity_key: entity.key, entity_type: entity.kind } : {}) };
        const native = nativeProject ? await nativeAstApi.source(astSelection(selection!), input, controller.signal) : await nativeAstApi.importedSource(importedSource!, input, controller.signal);
        if (!isCurrent()) return;
        setSource({ path, line, entity, content: native.selection.source, provenance: native.origin, native, request: id, observationSchema: nativeSchema.current ?? undefined });
      } catch (cause) {
        if (isCurrent()) setSourceError('Source unavailable: ' + (cause instanceof Error ? cause.message : 'Unable to read source'));
      } finally { if (isCurrent()) setSourceLoading(false); }
    },
    [nativeProject, selection, importedSource, currentScope, setSource, setSourceError, setSourceLoading, setSourceStopped, setError],
  );
  const choose = useCallback(
    (node: GraphNode | null) => {
      sourceAbort.current?.abort(); sourceRequest.current++; setSourceLoading(false); setSourceStopped(false); setSourceError(""); setFileRead(null);
      if (!node) {
        setSource(null);
        setSelected(null);
        return;
      }
      relationRequest.current++;
      setRelationLoading(false);
      setRelated([]);
      setSelected(node);
      setSource(null);
      setRelation("source");
      setView("map");
      if (node.file) void openFile(node.file, node.line);
      else {
        sourceRequest.current++;
        setSource(null);
        setSourceLoading(false);
      }
    },
    [setFileRead, openFile, setSelected, setRelationLoading, setRelated, setRelation, setView, setSource, setSourceError, setSourceLoading, setSourceStopped],
  );
  const targetTypes = (kind: string, edge: string) => {
    const endpoints = (schema.relationship_endpoints || []).filter(
      (e) => e.type === (kind === "impact" ? "CALLS" : edge),
    );
    let labels = new Set(selected ? [selected.label] : []);
    const result = new Set<string>();
    for (let hop = 0; hop < (kind === "impact" ? 2 : 1); hop++) {
      const next = new Set<string>();
      for (const e of endpoints) {
        if (labels.has(kind === "outgoing" ? e.from : e.to))
          next.add(kind === "outgoing" ? e.to : e.from);
      }
      next.forEach((label) => result.add(label));
      labels = next;
    }
    return [...result].sort();
  };
  const investigate = async (
    kind: string,
    edge = edgeType,
    requestedTarget?: string,
  ) => {
    setRelation(kind);
    setRelated([]);
    const id = ++relationRequest.current;
    if (kind === "source" || !selected) {
      setRelationLoading(false);
      return;
    }
    relationAbort.current?.abort(); const controller = new AbortController(); relationAbort.current = controller;
    const startedScope = currentScope;
    setRelationLoading(true);
    setError("");
    try {
      const anchor = schema.node_types?.find((t) => t.label === selected.label);
      const options = targetTypes(kind, edge);
      const targetLabel = requestedTarget || options[0];
      setTargetType(targetLabel || "");
      const target = schema.node_types?.find((t) => t.label === targetLabel);
      if (!anchor?.identity_property || !schema.relationship_endpoints)
        throw new Error(
          "This index does not expose identity and relationship metadata. Use Query lab to inspect its schema.",
        );
      if (!targetLabel) return;
      if (!target?.identity_property)
        throw new Error(
          "The related entity has no declared identity in this schema.",
        );
      const edgeName = kind === "impact" ? "CALLS" : edge;
      const identifiers = [
        anchor.label,
        anchor.identity_property,
        target.label,
        target.identity_property,
        edgeName,
        ...target.properties,
      ];
      if (identifiers.some((value) => !/^[A-Za-z_][A-Za-z0-9_]*$/.test(value)))
        throw new Error("This schema requires an explicit query in Query lab.");
      {
        if ((nativeProject ? !selection : !importedSource) || !nativeSchema.current) throw new Error('The primary key is unavailable in this result. Reload the sample or query before tracing relationships.');
        const observed = await resolveNativeNode(selected, nativeSchema.current, nativeProject ? astSelection(selection!) : importedSource!, controller.signal);
        const pattern = kind === 'outgoing'
          ? '(anchor:' + anchor.label + ')-[:' + edgeName + ']->(n:' + target.label + ')'
          : '(n:' + target.label + ')-[:' + edgeName + (kind === 'impact' ? '*1..2' : '') + ']->(anchor:' + anchor.label + ')';
        const query = 'MATCH ' + pattern + ' WHERE anchor.' + anchor.identity_property + ' = $anchor_key RETURN DISTINCT n LIMIT 100';
        const parameters = { anchor_key: observed.nativeKey!.type === 'int64' ? BigInt(observed.nativeKey!.value) : observed.nativeKey!.value };
        const raw = nativeProject ? await nativeAstApi.query(astSelection(selection!), query, parameters, controller.signal) : await nativeAstApi.importedQuery(importedSource!, query, parameters, controller.signal);
        if (id === relationRequest.current && scope.current === startedScope && !controller.signal.aborted) setRelated(nativeQueryView(raw, nativeSchema.current).nodes);
        return;
      }

    } catch (e) {
      if (id === relationRequest.current && scope.current === startedScope && !controller.signal.aborted) { if (nativeProject) invalidateTaskWorkspace(e, "ast"); setError((e as Error).message); }
    } finally {
      if (id === relationRequest.current && scope.current === startedScope && !controller.signal.aborted) setRelationLoading(false);
    }
  };
  const sample = useCallback(async () => {
    beginResult("Index sample");
    lastSearch.current = "";
    setSearching(false);
    lastGraphQuery.current = {};
    const id = ++graphRequest.current;
    const startedScope = currentScope;
    const controller = new AbortController(); graphAbort.current = controller;
    setQueryLoading(true);
    setError("");
    try {
      const data = await readGraph(undefined, controller.signal);
      if (id === graphRequest.current && scope.current === startedScope && !controller.signal.aborted) {
        setNodes(data.nodes || []);
        setLinks(data.links || []);
        setTabular(null);
      }
    } catch (e) {
      if (id === graphRequest.current && scope.current === startedScope && !controller.signal.aborted) { invalidateTaskWorkspace(e, "ast"); setError((e as Error).message); }
    } finally {
      if (id === graphRequest.current && scope.current === startedScope && !controller.signal.aborted) setQueryLoading(false);
    }
  }, [beginResult, currentScope, readGraph, setSearching, setQueryLoading, setError, setNodes, setLinks, setTabular]);
  useEffect(() => {
    if (view !== "map" || (nativeProject ? !selection : !importedSource) || initialSample.current === currentScope || !nativeSchema.current) return;
    initialSample.current = currentScope;
    if (!lastGraphQuery.current && !lastSearch.current) void sample();
  }, [view, currentScope, projectDir, projectId, schema, nativeProject, selection, importedSource, sample]);
  const refreshReaders = async () => {
    const startedScope = currentScope;
    { setResultVersion(version => version + 1); graphAbort.current?.abort(); relationAbort.current?.abort(); sourceAbort.current?.abort(); graphRequest.current++; relationRequest.current++; sourceRequest.current++; setNodes([]); setLinks([]); setTabular(null); setSelected(null); setSource(null); setRelated([]); setFileRead(null); }
    const startedRequest = graphRequest.current;
    schemaAbort.current?.abort(); const metadataController = new AbortController(); schemaAbort.current = metadataController;
    const metadata = async () => {
      let nextSchema: SchemaResponse;
      try { nextSchema = await readSchema(metadataController.signal); } catch (cause) { if (!metadataController.signal.aborted && scope.current === startedScope) { if (nativeProject) invalidateTaskWorkspace(cause, "ast"); setError((cause as Error).message); } throw cause; }
      if (metadataController.signal.aborted || scope.current !== startedScope) return;
      setSchema(nextSchema);
      if (!lastGraphQuery.current && !lastSearch.current && !selected) setError("");
    };
    const graph = async () => {
      if (!lastGraphQuery.current || graphRequest.current !== startedRequest) return;
      const request = ++graphRequest.current;
      const isCurrent = () => request === graphRequest.current && scope.current === startedScope;
      setQueryLoading(true);
      setRefreshingGraph(true);
      setNodes([]); setLinks([]); setTabular(null); setSelected(null); setSource(null); setRelated([]);
      graphAbort.current?.abort(); const controller = new AbortController(); graphAbort.current = controller;
      try {
        const data = await readGraph(lastGraphQuery.current.cypher_query, controller.signal);
        if (!isCurrent()) return;
        setError("");
        setNodes(data.nodes || []);
        setLinks(data.links || []);
        setTabular(data.tabular || null);
      } catch (e) {
        if (isCurrent() && !controller.signal.aborted) { invalidateTaskWorkspace(e, "ast"); setError((e as Error).message); throw e; }
      } finally {
        if (isCurrent()) { setQueryLoading(false); setRefreshingGraph(false); }
      }
    };
    if (!nativeProject) { const inventory = await imports.reload(true); if (!inventory || !inventory.imports.some(item => item.id === importedSource?.id && item.version === importedSource.version && item.context === importedSource.context) || scope.current !== startedScope) return; }
    await refreshAll([
      metadata().then(async () => {
        if (!metadataController.signal.aborted && scope.current === startedScope) await graph();
      }),
      lastSearch.current ? search(lastSearch.current, false) : Promise.resolve(),
    ]);
  };
  const sourceFailure = sourceError ? <WorkNotice tone="error" title="Source read failed">{sourceError}{source && <p>The previously loaded source remains visible.</p>}</WorkNotice> : null;
  const sourceDocument = source ? (
                      <>
                        <p className="source-provenance">
                          Source: {source.provenance}. {nativeProject ? source.native.origin === "indexed" ? "Captured indexed source of the selected PROJECT." : "Physical source read for the selected PROJECT." : "Source returned by the admitted imported reader."}
                        </p>
                        {source.native && <details><summary>Native Source result</summary><pre>{stringifyNativeJson(source.native, 2)}</pre></details>}
                        <div className="investigation-code">
                          <CodePanel
                            content={source.content}
                            filename={source.path}
                            highlightLine={source.line}
                            sourceSelection={source.native.selection}
                            onClose={() => {
                              cancelSource();
                              setSource(null);
                            }}
                          />
                        </div>
                        {source.observationSchema?.nodes.some(type => type.label === 'AstFileObservation') && <PublishedFileObservations key={currentScope + ':' + source.request} path={source.path} schema={source.observationSchema} read={readFileObservations} />}
                      </>
  ) : null;
  usePageRefresh(refreshReaders);
  if (contextURI.error) return <WorkNotice tone="error" title="AST context unavailable">{contextURI.error}</WorkNotice>;
  if (scopeError) return <WorkNotice tone="error" title="Imported working context unavailable">{scopeError}</WorkNotice>;
  if (nativeProject && !selection) return <TaskWorkspaceNotice module="ast" />;
  if (!nativeProject && !context) return <WorkEmpty title="Select an imported AST context" action={<button className="work-button" onClick={() => navigate("/ast/contexts?library=imported")}>Indexed contexts</button>}>{"Choose an installed code index to investigate."}{error && <p role="alert">{error}</p>}</WorkEmpty>;
  if (!nativeProject && imports.loading && !imports.items.length) return <LoadingSpinner label="Loading installed AST contexts…" />;
  if (!nativeProject && imports.error) return <WorkNotice tone="error" title="Installed AST contexts unavailable">{imports.error}<button className="work-button" onClick={() => void imports.refresh()}>Retry catalogue</button></WorkNotice>;
  return (
    <WorkPage className="code-investigation">
      <WorkHeader
        title="Code investigation"
        description={view === "map" ? "" : "Find the implementation. Read its source. Trace the relationships that inform a change."}
        actions={
          <button
            className="work-button"
            onClick={() => navigate("/ast/contexts?" + new URLSearchParams(physicalBinding ? { selection: encodeProjectLocator(physicalBinding), ...(!nativeProject ? { library: "imported" } : {}) } : { library: "imported" }))}
          >
            Indexed contexts
          </button>
        }
      />
      <div className="investigation-scope">
        <WorkBadge>AST</WorkBadge>
        <code>{context || "Project index"}</code>
        <span>
          {schema.nodes.reduce((n, s) => n + BigInt(s.count), 0n).toLocaleString()}{" "}
          indexed nodes
        </span>
        <span>{schema.backend}</span>
      </div>
      {!nativeProject && <div className="work-toolbar" aria-label="Installed AST context">
        <label className="work-field"><span>AST artifact and version</span><StyledSelect aria-label="AST artifact and version" value={context || ''} onChange={event => { const observed = imports.items.find(item => item.context === event.target.value); if (observed) navigate(importedAstHref(observed)); }}>
          {imports.items.map(item => <option key={stringifyNativeJson([item.id, item.version])} value={item.context}>{item.id + ' · ' + item.version}</option>)}
        </StyledSelect></label>
      </div>}
      <WorkTabs
        value={view}
        onChange={setView}
        items={[
          ["investigate", "Find & inspect"],
          ["query", "Query lab"],
          ["map", "Relationship map"],
        ]}
        label="Investigation tools"
      />
      {error && (
        <WorkNotice tone="error" title="Could not complete this request">
          {error}<button className="work-button" onClick={() => void refreshReaders().catch(() => {})}>Retry request</button>
        </WorkNotice>
      )}
      {view === "investigate" && (
        <>
          <form
            className="work-toolbar"
            onSubmit={(e) => {
              e.preventDefault();
              void search();
            }}
          >
            <WorkSearch
              label="Search indexed code"
              value={query}
              onChange={value => { clearSearch(); setQuery(value); }}
              placeholder="Symbol, path or implementation intent"
            />
            <label className="work-field"><span>Search mode</span><StyledSelect aria-label="Search mode" value={searchMode} onChange={event => { clearSearch(); setSearchMode(event.target.value as AstSearchMode); }}>{(['fts', 'semantic', 'vector', 'hybrid'] as const).map(mode => <option key={mode} value={mode}>{mode}</option>)}</StyledSelect></label>
            <label className="work-field"><span>Distance metric</span><StyledSelect aria-label="Distance metric" value={searchMetric} onChange={event => { clearSearch(); setSearchMetric(event.target.value as AstSearchMetric); }}>{(['cosine', 'l2', 'dot'] as const).map(metric => <option key={metric} value={metric}>{metric}</option>)}</StyledSelect></label>
            <label className="work-field"><span>Reranking</span><StyledSelect aria-label="AST reranking" value={rerank} onChange={event => { clearSearch(); setRerank(event.target.value as typeof rerank); }}><option value="configuration">Use configuration</option><option value="off">Off</option><option value="on">On</option></StyledSelect></label>
            <button
              className="work-button primary"
              disabled={searching || !query.trim()}
            >
              {searching ? "Searching…" : "Search index"}
            </button>
            <small>Up to 20 matches · selected context</small>
          </form>
          <WorkNotice title="Explore results in Relationship map">
            Search functions, types and files across the selected index. Results open in
            Relationship map, where you can inspect source and query relationships.
          </WorkNotice>
          {nativeProject && <WorkSection title="Read a source file" description="Enter a path within the selected project. The reader reports whether the content comes from the index or the file.">
            <form className="work-toolbar" onSubmit={event => { event.preventDefault(); if (!filePath.trim()) return; setSelected(null); setFileRead(filePath); void openFile(filePath); }}>
              <WorkSearch label="File path" value={filePath} onChange={setFilePath} placeholder="Path within the selected project" />
              <button className="work-button" disabled={!filePath.trim()}>Read source file</button>
            </form>
          </WorkSection>}
        </>
      )}
            {searching && <button type="button" className="work-button" onClick={() => { graphAbort.current?.abort(); graphRequest.current++; setSearching(false); setError('Search request stopped.'); }}>Cancel search</button>}
      {searchReply && <>
        {searchReply.status !== 'available' && <WorkNotice title="Search unavailable">{searchReply.status}</WorkNotice>}
        {searchReply.vector_status && <WorkNotice title="Partial search results">{'Vector component: ' + searchReply.vector_status + '. These are the published results returned for this request.'}</WorkNotice>}
        <details className="work-disclosure"><summary>Native search response</summary><pre className="work-code">{stringifyNativeJson(searchReply, 2)}</pre></details>
      </>}
      {fileRead && !selected && <section className="investigation-document" aria-label="File source" aria-busy={sourceLoading}>
        <header className="symbol-header"><div><h2>File source</h2><code>{fileRead}</code></div><button className="work-button" onClick={() => { cancelSource(); setFileRead(null); setSource(null); }}>Close file</button></header>
        {sourceFailure}
        {sourceLoading ? <><LoadingSpinner label="Reading source…" /><button className="work-button" onClick={cancelSource}>Cancel source</button></> : sourceDocument || <WorkEmpty title={sourceStopped ? "Source read stopped" : "No source open"} action={<button className="work-button" onClick={() => void openFile(fileRead)}>Load source</button>}>Read the selected file again.</WorkEmpty>}
      </section>}
      <div hidden={view !== "query"}>
          <div className="query-lab-layout">
            <WorkSection
              title="A reproducible question"
              description="Run a read-only Cypher query. AI can draft it; you review and execute it."
            >
              <QueryBar
                contextId={context}
                selection={physicalBinding}
                nativeProject={nativeProject}
                importedSource={importedSource}
                projectDir={projectDir}
                projectId={projectId}
                loading={queryLoading && !refreshingGraph}
                setLoading={setQueryLoading}
                onQueryStart={(executedQuery) => {
                  const request = ++graphRequest.current;
                  const startedScope = currentScope;
                  lastSearch.current = "";
                  lastGraphQuery.current = { cypher_query: executedQuery };
                  setSearching(false);
                  beginResult("Cypher query result", "query");
                  return () => request === graphRequest.current && scope.current === startedScope;
                }}
                onQueryAbort={controller => { graphAbort.current = controller; }}
                onQueryError={setError}
                onQueryResult={(value, executedQuery) => {
                  const renderer = nativeQueryView(value as AstQuery, nativeSchema.current ?? undefined);
                  graphRequest.current++;
                  setQueryLoading(false);
                  lastGraphQuery.current = { cypher_query: executedQuery };
                  const r = renderer as {
                    nodes?: GraphNode[];
                    links?: GraphEdge[];
                    tabular?: { columns: string[]; rows: unknown[][] };
                  };
                  setNodes(r.nodes || []);
                  setLinks(r.links || []);
                  setTabular(r.tabular || null);
                  setView("map");
                }}
              />
            </WorkSection>
            <aside className="work-panel">
              <h2>Available vocabulary</h2>
              <FactList
                items={schema.nodes.map((n) => [
                  n.label,
                  n.count.toLocaleString(),
                ])}
              />
              <p className="text-xs mt-4">
                {schema.edges.map((e) => e.type).join(" · ") ||
                  "No relationship types indexed."}
              </p>
            </aside>
          </div>
      </div>
      <div hidden={view !== "map"}>
        <div className="map-workspace">
          <div className="relationship-intro">
            <p>Choose a boundary. Follow a relationship. Inspect the implementation.</p>
          <button
            className="work-button"
            disabled={queryLoading || searching}
            onClick={() => void sample()}
          >
            {queryLoading ? "Loading…" : "Load index sample"}
          </button>
          {queryLoading && <button className="work-button" onClick={() => { graphAbort.current?.abort(); graphRequest.current++; setQueryLoading(false); setRefreshingGraph(false); setError("Graph request stopped."); }}>Cancel graph request</button>}
          </div>
          <div role="status" aria-live="polite">
            {searching ? <LoadingSpinner label="Searching index…" /> : queryLoading ? <LoadingSpinner label="Loading indexed relationships…" /> : !error && resultOrigin && <p>{resultOrigin} · {nodes.length} entities{tabular ? ` · ${tabular.rows.length} result rows` : ""}</p>}
          </div>
          {!queryLoading && !searching && !error && !nodes.length && !tabular && resultKind === "search" && searchReply?.status === "available" && <WorkEmpty title="No indexed matches">Try a symbol name or a shorter phrase in Find & inspect.</WorkEmpty>}
          {(nodes.length > 0 || (!tabular && resultKind !== "search")) &&
          <RelationshipExplorer
            key={currentScope + ":" + resultVersion}
            nodes={nodes}
            links={links}
            loading={queryLoading || searching}
            error={Boolean(error)}
            onInspect={choose}
            loadNeighborhood={exploreNeighborhood}
            scope={entityScope ?? undefined}
          />}
          {!queryLoading && !searching && !error && tabular && <details className="work-panel" open={!nodes.length}>
            <summary>Query rows · {tabular.rows.length}</summary>
            <p>Returned values are preserved as a table. Scalar values do not imply graph relationships.</p>
            <div className="query-table"><TabularResults columns={tabular.columns} rows={tabular.rows} onClose={() => setTabular(null)} /></div>
          </details>}
          {selected && (
            <section
              ref={inspection}
              tabIndex={-1}
              className="investigation-document"
              aria-label="Symbol investigation"
            >
              {selected ? (
                <>
                  <header className="symbol-header">
                    <div>
                      <small>{selected.label}</small>
                      <h2>{selected.name || selected.label}</h2>
                      <code>
                        {selected.file}
                        {selected.line ? ":" + selected.line : ""}
                      </code>
                    </div>
                    <button className="work-button" onClick={() => { cancelSource(); setSelected(null); setSource(null); relationRequest.current++; }}>Close inspection</button>
                  </header>
                  <NativeEntityDetails key={selected.id} node={selected} scope={entityScope ?? undefined} />
                  <WorkTabs
                    value={relation}
                    onChange={(id) => void investigate(id)}
                    items={[
                      ["source", "Source"],
                      ["incoming", "Incoming"],
                      ["outgoing", "Outgoing"],
                      ["impact", "Potential impact"],
                    ]}
                    label="Symbol evidence"
                  />
                  {relation === "source" && sourceFailure}
                  {relation === "source" ? (
                    sourceLoading ? (
                      <><LoadingSpinner label="Reading source…" /><button className="work-button" onClick={cancelSource}>Cancel source</button></>
                    ) : source ? (
                      <>
                        {selected.nativeKey && selected.file && <div className="work-actions"><button className="work-button" onClick={() => void openFile(selected.file!, selected.line, { key: selected.nativeKey!, kind: selected.label })}>Read entity source</button><button className="work-button" onClick={() => void openFile(selected.file!, selected.line)}>Read complete file</button></div>}
                        {sourceDocument}
                      </>
                    ) : (
                      <WorkEmpty
                        title={sourceStopped ? "Source read stopped" : "No source open"}
                        action={
                          selected.file ? (
                            <button
                              className="work-button"
                              onClick={() =>
                                void openFile(selected.file!, selected.line)
                              }
                            >
                              Load source
                            </button>
                          ) : undefined
                        }
                      >
                        Some indexed entities have no source file.
                      </WorkEmpty>
                    )
                  ) : (
                    <>
                      {relation !== "impact" && (
                        <label className="work-field mb-4">
                          <span>Relationship type</span>
                          <StyledSelect
                            value={edgeType}
                            onChange={(e) => {
                              setEdgeType(e.target.value);
                              void investigate(relation, e.target.value);
                            }}
                          >
                            {Array.from(
                              new Set([
                                "CALLS",
                                ...schema.edges.map((e) => e.type),
                              ]),
                            ).map((t) => (
                              <option key={t}>{t}</option>
                            ))}
                          </StyledSelect>
                        </label>
                      )}
                      {targetTypes(relation, edgeType).length > 0 && (
                        <label className="work-field mb-4">
                          <span>Related entity type</span>
                          <StyledSelect
                            value={targetType}
                            onChange={(e) =>
                              void investigate(
                                relation,
                                edgeType,
                                e.target.value,
                              )
                            }
                          >
                            {targetTypes(relation, edgeType).map((label) => (
                              <option key={label}>{label}</option>
                            ))}
                          </StyledSelect>
                        </label>
                      )}
                      <WorkNotice
                        title={
                          relation === "impact"
                            ? "Potential impact · up to two incoming call hops"
                            : "Indexed relationships"
                        }
                      >
                        {relation === "impact"
                          ? "Reachability identifies candidates for review, not guaranteed runtime impact."
                          : "Relationships reflect the current index. Dynamic and unresolved calls may be absent."}{" "}
                        Results are limited to 100 nodes.
                      </WorkNotice>
                      {relationLoading ? (
                        <LoadingSpinner label="Following relationships…" />
                      ) : related.length ? (
                        <>
                          {related.map((n) => (
                            <RecordLink
                              key={n.id}
                              title={n.name || n.id}
                              meta={
                                n.label + " · " + (n.file || "No source path")
                              }
                              onClick={() => choose(n)}
                            />
                          ))}
                        </>
                      ) : (
                        <WorkEmpty title="No indexed relationships found">
                          This does not prove that runtime relationships do not
                          exist.
                        </WorkEmpty>
                      )}
                    </>
                  )}
                </>
              ) : (
                <WorkEmpty title="Inspect an implementation">
                  Select a search result to read its source and investigate how
                  it connects to the system.
                </WorkEmpty>
              )}
            </section>
          )}
        </div>
      </div>
    </WorkPage>
  );
}
