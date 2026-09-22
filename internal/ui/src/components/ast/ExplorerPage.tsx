import { StyledSelect } from "@/components/shared/StyledSelect";
import { usePageRefresh, refreshAll } from "@/components/layout/WorkspaceRefresh";
import { useEffect, useLayoutEffect, useState, useRef, useCallback } from "react";
import { useParams, useNavigate } from "react-router-dom";
import {
  astApi,
  type GraphNode,
  type GraphEdge,
  type SchemaResponse,
} from "@/api/ast";
import { hubApi, type ProjectContextEntry } from "@/api/hub";
import { loadNeighborhood, type Neighborhood, type NeighborhoodLoader } from "./neighborhood";
import { RelationshipExplorer } from "./RelationshipExplorer";
import { QueryBar } from "./QueryBar";
import { CodePanel } from "./CodePanel";
import { TabularResults } from "./TabularResults";
import { useAppStore } from "@/store/appStore";
import { projectRequestScope } from "@/lib/projectScope";
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
const quote = (s: string) =>
  "'" + s.replace(/\\/g, "\\\\").replace(/'/g, "\\'") + "'";

export default function ExplorerPage() {
  const { contextId } = useParams<{ contextId: string }>();
  const context = contextId ? decodeURIComponent(contextId) : undefined;
  const { activeProjectKey, activeProjectOrigin, activeProjectDir, activeProjectId } = useAppStore();
  const { key: projectKey, projectDir, projectId, remote } = projectRequestScope({
    activeProjectKey, activeProjectOrigin, activeProjectDir, activeProjectId,
  });
  const navigate = useNavigate();
  const [remoteContexts, setRemoteContexts] = useState<ProjectContextEntry[]>([]);
  const [view, setView] = useState("investigate");
  const [query, setQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState("");
  const [selected, setSelected] = useState<GraphNode | null>(null);
  const [source, setSource] = useState<{
    path: string;
    content: string;
    line?: number;
    provenance: string;
  } | null>(null);
  const [sourceLoading, setSourceLoading] = useState(false);
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
  const currentScope = projectKey + "|" + (context || "");
  useLayoutEffect(() => { scope.current = currentScope; }, [currentScope]);
  const [dataScope, setDataScope] = useState(currentScope);
  if (dataScope !== currentScope) {
    setDataScope(currentScope);
    setSelected(null);
    setSource(null);
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
    setSourceLoading(false);
    setRelationLoading(false);
    setSchema({
      nodes: [],
      edges: [],
      node_labels: [],
      edge_types: [],
      langs: [],
      backend: "",
    });
    setRemoteContexts([]);
  }
  useEffect(() => {
    sourceRequest.current++;
    relationRequest.current++;
    graphRequest.current++;
    lastSearch.current = "";
    lastGraphQuery.current = null;
    initialSample.current = "";
    let active = true;
    const schemaRequest = projectId
      ? astApi.getSchema(context, projectDir, projectId)
      : astApi.getSchema(context, projectDir);
    schemaRequest
      .then((s) => {
        if (active) setSchema(s);
      })
      .catch((e) => {
        if (active) setError(e.message);
      });
    return () => {
      active = false;
    };
  }, [context, projectDir, projectId]);
  useEffect(() => {
    if (!remote || !projectId) return;
    let active = true;
    hubApi.getProjectContexts(projectId).then(response => {
      if (!active) return;
      const entries = response.entries.filter(entry => entry.type === "ast" && entry.qualified_latest);
      setRemoteContexts(entries);
      if (!context && entries[0]?.qualified_latest) navigate(`/ast/explorer/${encodeURIComponent(entries[0].qualified_latest)}`, { replace: true });
    }).catch(e => { if (active) setError((e as Error).message); });
    return () => { active = false; };
  }, [remote, projectId, context, navigate]);
  const exploreNeighborhood: NeighborhoodLoader = useCallback(
    (node: GraphNode, previous?: Neighborhood, signal?: AbortSignal) => loadNeighborhood(node, schema, context, projectDir, previous, signal, projectId),
    [schema, context, projectDir, projectId],
  );
  const beginResult = (origin: string, kind: "sample" | "search" | "query" = "sample") => {
    setResultKind(kind);
    setRefreshingGraph(false);
    setView("map");
    setResultOrigin(origin);
    setResultVersion(version => version + 1);
    setNodes([]); setLinks([]); setTabular(null);
    setSelected(null); setSource(null); setRelated([]);
    sourceRequest.current++; relationRequest.current++;
    setSourceLoading(false); setRelationLoading(false);
    setError("");
  };
  const search = async (text = query.trim(), present = true) => {
    if (!text) return;
    lastSearch.current = text;
    lastGraphQuery.current = null;
    const id = ++graphRequest.current;
    const startedScope = currentScope;
    if (present) beginResult("Search results · " + text, "search");
    setQueryLoading(false);
    setSearching(true);
    setError("");
    try {
      const rows = projectId
        ? await astApi.search(text, context, projectDir, projectId)
        : await astApi.search(text, context, projectDir);
      if (id !== graphRequest.current || scope.current !== startedScope) return;
      setNodes((rows || []).map((r, index) => ({
        id: JSON.stringify([r.Type, r.Path, r.Line, r.Name]),
        name: r.Name, label: r.Type, type: r.Type, file: r.Path, line: r.Line,
        properties: { search_rank: index + 1, docstring: r.Docstring, search_type: r.SearchType, relevance_score: r.RelevanceScore },
      })));
      setLinks([]); setTabular(null);
    } catch (e) {
      if (id === graphRequest.current && scope.current === startedScope) setError((e as Error).message);
    } finally {
      if (id === graphRequest.current && scope.current === startedScope) setSearching(false);
    }
  };
  const openFile = useCallback(
    async (path: string, line?: number) => {
      const id = ++sourceRequest.current;
      setSource(null);
      setSourceLoading(true);
      setError("");
      try {
        const data = projectId
          ? await astApi.getFile(path, context, projectDir, projectId)
          : await astApi.getFile(path, context, projectDir);
        if (id === sourceRequest.current)
          setSource({
            path,
            line,
            content: data.content,
            provenance: data.source || "indexed",
          });
      } catch (e) {
        if (id === sourceRequest.current)
          setError("Indexed source unavailable: " + (e as Error).message);
      } finally {
        if (id === sourceRequest.current) setSourceLoading(false);
      }
    },
    [context, projectDir, projectId],
  );
  const choose = useCallback(
    (node: GraphNode | null) => {
      if (!node) {
        setSelected(null);
        return;
      }
      relationRequest.current++;
      setRelationLoading(false);
      setRelated([]);
      setSelected(node);
      setRelation("source");
      setView("map");
      if (node.file) void openFile(node.file, node.line);
      else {
        sourceRequest.current++;
        setSource(null);
        setSourceLoading(false);
      }
    },
    [openFile],
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
      let identity = selected.properties?.[anchor.identity_property];
      if (identity == null && anchor.identity_property === "path")
        identity = selected.file;
      if (identity == null || identity === "") {
        const filters: string[] = [];
        if (anchor.properties.includes("name"))
          filters.push("n.name = " + quote(selected.name));
        if (anchor.properties.includes("path") && selected.file)
          filters.push("n.path = " + quote(selected.file));
        if (anchor.properties.includes("line_number") && selected.line)
          filters.push("n.line_number = " + selected.line);
        if (!filters.length)
          throw new Error(
            "Search an indexed entity with a stable identity before tracing relationships.",
          );
        const resolved = await astApi.getGraph({
          context,
          project_dir: projectDir,
          ...(projectId ? { project_id: projectId } : {}),
          cypher_query:
            "MATCH (n:" +
            anchor.label +
            ") WHERE " +
            filters.join(" AND ") +
            " RETURN n LIMIT 2",
        });
        if (id !== relationRequest.current) return;
        if (resolved.nodes.length !== 1)
          throw new Error(
            resolved.nodes.length
              ? "Multiple indexed symbols match this location. Refine the symbol in Query lab before tracing relationships."
              : "This symbol is no longer present in the current index. Refresh the search before tracing relationships.",
          );
        identity = resolved.nodes[0].properties?.[anchor.identity_property];
        if (identity == null || identity === "")
          throw new Error(
            "The indexed symbol has no stable identity. Renderer IDs cannot identify indexed entities.",
          );
        setSelected((previous) =>
          previous?.id === selected.id
            ? {
                ...previous,
                properties: {
                  ...previous.properties,
                  [anchor.identity_property]: identity,
                },
              }
            : previous,
        );
      }
      const pattern =
        kind === "outgoing"
          ? "(anchor:" +
            anchor.label +
            ")-[:" +
            edgeName +
            "]->(n:" +
            target.label +
            ")"
          : "(n:" +
            target.label +
            ")-[:" +
            edgeName +
            (kind === "impact" ? "*1..2" : "") +
            "]->(anchor:" +
            anchor.label +
            ")";
      const fields = ["n." + target.identity_property + " AS identity"];
      for (const [property, alias] of [
        ["name", "name"],
        ["path", "path"],
        ["line_number", "line"],
      ])
        if (target.properties.includes(property))
          fields.push("n." + property + " AS " + alias);
      const data = await astApi.getGraph({
        context,
        project_dir: projectDir,
        ...(projectId ? { project_id: projectId } : {}),
        cypher_query:
          "MATCH " +
          pattern +
          " WHERE anchor." +
          anchor.identity_property +
          " = " +
          quote(String(identity)) +
          " RETURN DISTINCT " +
          fields.join(", ") +
          " LIMIT 100",
      });
      if (id === relationRequest.current) {
        const table = data.tabular;
        setRelated(
          table
            ? table.rows.map((row) => {
                const item = Object.fromEntries(
                  table.columns.map((c, i) => [c, row[i]]),
                );
                return {
                  id: target.label + ":" + String(item.identity),
                  name: String(item.name || item.identity),
                  file: String(item.path || ""),
                  line: Number(item.line) || undefined,
                  label: target.label,
                  type: target.label,
                  properties: { [target.identity_property]: item.identity },
                };
              })
            : data.nodes || [],
        );
      }
    } catch (e) {
      if (id === relationRequest.current) setError((e as Error).message);
    } finally {
      if (id === relationRequest.current) setRelationLoading(false);
    }
  };
  const sample = async () => {
    beginResult("Index sample");
    lastSearch.current = "";
    setSearching(false);
    lastGraphQuery.current = {};
    const id = ++graphRequest.current;
    setQueryLoading(true);
    setError("");
    try {
      const data = await astApi.getGraph({ context, project_dir: projectDir, ...(projectId ? { project_id: projectId } : {}) });
      if (id === graphRequest.current) {
        setNodes(data.nodes || []);
        setLinks(data.links || []);
        setTabular(null);
      }
    } catch (e) {
      if (id === graphRequest.current) setError((e as Error).message);
    } finally {
      if (id === graphRequest.current) setQueryLoading(false);
    }
  };
  useEffect(() => {
    if (view !== "map" || (!projectDir && !projectId) || initialSample.current === currentScope) return;
    initialSample.current = currentScope;
    if (!lastGraphQuery.current && !lastSearch.current) void sample();
  }, [view, currentScope, projectDir, projectId]);
  usePageRefresh(async () => {
    const startedScope = currentScope;
    const metadata = async () => {
      const nextSchema = projectId
        ? await astApi.getSchema(context, projectDir, projectId)
        : await astApi.getSchema(context, projectDir);
      if (scope.current !== startedScope) return;
      setSchema(nextSchema);
    };
    const graph = async () => {
      if (!lastGraphQuery.current) return;
      const request = ++graphRequest.current;
      const isCurrent = () => request === graphRequest.current && scope.current === startedScope;
      setQueryLoading(true);
      setRefreshingGraph(true);
      try {
        const data = await astApi.getGraph({ context, project_dir: projectDir, ...(projectId ? { project_id: projectId } : {}), ...lastGraphQuery.current });
        if (!isCurrent()) return;
        setError("");
        setNodes(data.nodes || []);
        setLinks(data.links || []);
        setTabular(data.tabular || null);
      } catch (e) {
        if (isCurrent()) { setError((e as Error).message); throw e; }
      } finally {
        if (isCurrent()) { setQueryLoading(false); setRefreshingGraph(false); }
      }
    };
    await refreshAll([
      metadata(), graph(),
      lastSearch.current ? search(lastSearch.current, false) : Promise.resolve(),
      source ? openFile(source.path, source.line) : Promise.resolve(),
      selected && relation !== "source" ? investigate(relation, edgeType, targetType) : Promise.resolve(),
    ]);
  });
  const selectedRemoteContext = remoteContexts.find(entry =>
    context?.startsWith(entry.id + "@"));
  const selectedRemoteContextID = selectedRemoteContext?.id || "";
  const selectedRemoteLatest = selectedRemoteContext?.qualified_latest || "";
  return (
    <WorkPage className="code-investigation">
      <WorkHeader
        title="Code investigation"
        description={view === "map" ? "" : "Find the implementation. Read its source. Trace the relationships that inform a change."}
        actions={
          <button
            className="work-button"
            onClick={() => navigate("/ast/contexts")}
          >
            Indexed contexts
          </button>
        }
      />
      <div className="investigation-scope">
        <WorkBadge>AST</WorkBadge>
        <code>{context || "Project index"}</code>
        <span>
          {schema.nodes.reduce((n, s) => n + s.count, 0).toLocaleString()}{" "}
          indexed nodes
        </span>
        <span>{schema.backend}</span>
      </div>
      {remote && <div className="work-toolbar" aria-label="Published AST context">
        <label className="work-field">
          <span>AST artifact</span>
          <StyledSelect
            aria-label="AST artifact"
            value={selectedRemoteContext?.id || ""}
            onChange={(e) => {
              const entry = remoteContexts.find(item => item.id === e.target.value);
              if (entry?.qualified_latest) navigate(`/ast/explorer/${encodeURIComponent(entry.qualified_latest)}`);
            }}
          >
            {!remoteContexts.length && <option value="">No published artifacts</option>}
            {remoteContexts.map(entry => <option key={entry.id} value={entry.id}>{entry.name || entry.id}</option>)}
          </StyledSelect>
        </label>
        <label className="work-field">
          <span>Published version</span>
          <StyledSelect
            aria-label="AST version"
            value={context || ""}
            disabled={!selectedRemoteContext}
            onChange={(e) => navigate(`/ast/explorer/${encodeURIComponent(e.target.value)}`)}
          >
            {selectedRemoteContext?.qualified_latest &&
              <option value={selectedRemoteContext.qualified_latest}>Latest · {selectedRemoteContext.latest}</option>}
            {(selectedRemoteContext?.versions || []).filter(version =>
              `${selectedRemoteContextID}@${version}` !== selectedRemoteLatest)
              .map(version => <option key={version} value={`${selectedRemoteContextID}@${version}`}>{version}</option>)}
          </StyledSelect>
          {context && <small>Reading exact context <code>{context}</code> from Hub</small>}
        </label>
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
          {error}
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
              onChange={setQuery}
              placeholder="Symbol, path or implementation intent"
            />
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
        </>
      )}
      <div hidden={view !== "query"}>
          <div className="query-lab-layout">
            <WorkSection
              title="A reproducible question"
              description="Run a read-only Cypher query. AI can draft it; you review and execute it."
            >
              <QueryBar
                key={currentScope}
                contextId={context}
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
                onQueryError={setError}
                onQueryResult={(value, executedQuery) => {
                  graphRequest.current++;
                  setQueryLoading(false);
                  lastGraphQuery.current = { cypher_query: executedQuery };
                  const r = value as {
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
          </div>
          <div role="status" aria-live="polite">
            {searching ? <LoadingSpinner label="Searching index…" /> : queryLoading ? <LoadingSpinner label="Loading indexed relationships…" /> : !error && resultOrigin && <p>{resultOrigin} · {nodes.length} entities{tabular ? ` · ${tabular.rows.length} result rows` : ""}</p>}
          </div>
          {!queryLoading && !searching && !error && !nodes.length && !tabular && resultKind === "search" && <WorkEmpty title="No indexed matches">Try a symbol name or a shorter phrase in Find & inspect.</WorkEmpty>}
          {(nodes.length > 0 || (!tabular && resultKind !== "search")) &&
          <RelationshipExplorer
            key={currentScope + ":" + resultVersion}
            nodes={nodes}
            links={links}
            loading={queryLoading || searching}
            error={Boolean(error)}
            onInspect={choose}
            loadNeighborhood={exploreNeighborhood}
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
                      <h2>{selected.name}</h2>
                      <code>
                        {selected.file}
                        {selected.line ? ":" + selected.line : ""}
                      </code>
                    </div>
                    <button className="work-button" onClick={() => { setSelected(null); setSource(null); sourceRequest.current++; relationRequest.current++; }}>Close inspection</button>
                  </header>
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
                  {relation === "source" ? (
                    sourceLoading ? (
                      <LoadingSpinner label="Loading indexed source…" />
                    ) : source ? (
                      <>
                        <p className="source-provenance">
                          Source: {source.provenance}. This is the indexed
                          snapshot of the selected context.
                        </p>
                        <div className="investigation-code">
                          <CodePanel
                            content={source.content}
                            filename={source.path}
                            highlightLine={source.line ?? null}
                            onClose={() => {
                              sourceRequest.current++;
                              setSource(null);
                            }}
                          />
                        </div>
                      </>
                    ) : (
                      <WorkEmpty
                        title="No source open"
                        action={
                          selected.file ? (
                            <button
                              className="work-button"
                              onClick={() =>
                                void openFile(selected.file!, selected.line)
                              }
                            >
                              Load indexed source
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
