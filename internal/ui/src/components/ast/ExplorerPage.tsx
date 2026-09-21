import { StyledSelect } from "@/components/shared/StyledSelect";
import { usePageRefresh, refreshAll } from "@/components/layout/WorkspaceRefresh";
import { useEffect, useState, useRef, useCallback } from "react";
import { useParams, useNavigate } from "react-router-dom";
import {
  astApi,
  type GraphNode,
  type GraphEdge,
  type CodeSearchResult,
  type SchemaResponse,
} from "@/api/ast";
import { GraphCanvas, type GraphCanvasRef } from "./GraphCanvas";
import { QueryBar } from "./QueryBar";
import { SchemaPanel } from "./SchemaPanel";
import { CodePanel } from "./CodePanel";
import { TabularResults } from "./TabularResults";
import { NodeTree } from "./NodeTree";
import { useAppStore } from "@/store/appStore";
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
const LS = {
  get<T>(key: string, fallback: T): T {
    try {
      const v = localStorage.getItem(key);
      return v ? JSON.parse(v) : fallback;
    } catch {
      return fallback;
    }
  },
  set(key: string, value: unknown) {
    try {
      localStorage.setItem(key, JSON.stringify(value));
    } catch {
      /* optional preferences */
    }
  },
};
const quote = (s: string) =>
  "'" + s.replace(/\\/g, "\\\\").replace(/'/g, "\\'") + "'";

export default function ExplorerPage() {
  const { contextId } = useParams<{ contextId: string }>();
  const context = contextId ? decodeURIComponent(contextId) : undefined;
  const { activeProjectDir } = useAppStore();
  const navigate = useNavigate();
  const projectDir = activeProjectDir || undefined;
  const [view, setView] = useState("investigate");
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<CodeSearchResult[]>([]);
  const [searched, setSearched] = useState(false);
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
  const [queryRan, setQueryRan] = useState(false);
  const [projectRoot, setProjectRoot] = useState("");
  const searchRequest = useRef(0),
    sourceRequest = useRef(0),
    relationRequest = useRef(0),
    graphRequest = useRef(0);
  const lastSearch = useRef("");
  const initialSample = useRef("");
  const lastGraphQuery = useRef<{ cypher_query?: string } | null>(null);
  const scope = useRef("");
  const currentScope = (projectDir || "") + "|" + (context || "");
  scope.current = currentScope;
  useEffect(() => {
    searchRequest.current++;
    sourceRequest.current++;
    relationRequest.current++;
    graphRequest.current++;
    lastSearch.current = "";
    lastGraphQuery.current = null;
    initialSample.current = "";
    setResults([]);
    setSearched(false);
    setSelected(null);
    setSource(null);
    setRelated([]);
    setNodes([]);
    setLinks([]);
    setTabular(null);
    setQueryRan(false);
    setQueryLoading(false);
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
    let active = true;
    astApi
      .getSchema(context, projectDir)
      .then((s) => {
        if (active) setSchema(s);
      })
      .catch((e) => {
        if (active) setError(e.message);
      });
    astApi
      .getContexts(projectDir)
      .then((c) => {
        if (active) setProjectRoot(c.project_root || "");
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [context, projectDir]);
  const search = async (text = query.trim()) => {
    if (!text) return;
    lastSearch.current = text;
    const id = ++searchRequest.current;
    setSearching(true);
    setError("");
    setSearched(true);
    setResults([]);
    try {
      const rows = await astApi.search(text, context, projectDir);
      if (id === searchRequest.current) setResults(rows || []);
    } catch (e) {
      if (id === searchRequest.current) setError((e as Error).message);
    } finally {
      if (id === searchRequest.current) setSearching(false);
    }
  };
  const openFile = useCallback(
    async (path: string, line?: number) => {
      const id = ++sourceRequest.current;
      setSource(null);
      setSourceLoading(true);
      setError("");
      try {
        const data = await astApi.getFile(path, context, projectDir);
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
    [context, projectDir],
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
      setView("investigate");
      if (node.file) void openFile(node.file, node.line);
      else {
        sourceRequest.current++;
        setSource(null);
        setSourceLoading(false);
      }
    },
    [openFile],
  );
  const selectResult = (r: CodeSearchResult) =>
    choose({
      id: r.Path + ":" + r.Line + ":" + r.Name,
      name: r.Name,
      label: r.Type,
      type: r.Type,
      file: r.Path,
      line: r.Line,
    });
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
    lastGraphQuery.current = {};
    const id = ++graphRequest.current;
    setQueryLoading(true);
    setError("");
    try {
      const data = await astApi.getGraph({ context, project_dir: projectDir });
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
    if (view !== "map" || !projectDir || initialSample.current === currentScope) return;
    initialSample.current = currentScope;
    if (!lastGraphQuery.current) void sample();
  }, [view, currentScope, projectDir]);
  usePageRefresh(async () => {
    const startedScope = currentScope;
    const metadata = async () => {
      const [nextSchema, contexts] = await refreshAll([
        astApi.getSchema(context, projectDir), astApi.getContexts(projectDir),
      ] as const);
      if (scope.current !== startedScope) return;
      setSchema(nextSchema);
      setProjectRoot(contexts.project_root || "");
    };
    const graph = async () => {
      if (!lastGraphQuery.current) return;
      const request = graphRequest.current;
      const data = await astApi.getGraph({ context, project_dir: projectDir, ...lastGraphQuery.current });
      if (request !== graphRequest.current || scope.current !== startedScope) return;
      setNodes(data.nodes || []);
      setLinks(data.links || []);
      setTabular(data.tabular || null);
    };
    await refreshAll([
      metadata(), graph(),
      lastSearch.current ? search(lastSearch.current) : Promise.resolve(),
      source ? openFile(source.path, source.line) : Promise.resolve(),
      selected && relation !== "source" ? investigate(relation, edgeType, targetType) : Promise.resolve(),
    ]);
  });
  return (
    <WorkPage className="code-investigation">
      <WorkHeader
        title="Code investigation"
        description="Find the implementation. Read its source. Trace the relationships that inform a change."
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
          <div className="investigation-layout">
            <section
              aria-label="Code search results"
              className="investigation-results"
            >
              <h2>{searched ? "Search results" : "Start with a question"}</h2>
              {searching ? (
                <LoadingSpinner label="Searching index…" />
              ) : (
                results.map((r, i) => (
                  <RecordLink
                    key={r.Path + ":" + r.Line + ":" + i}
                    title={r.Name}
                    selected={
                      selected?.name === r.Name &&
                      selected?.file === r.Path &&
                      selected?.line === r.Line
                    }
                    meta={
                      <>
                        {r.Type} · {r.Path}:{r.Line}
                        <br />
                        {r.Docstring || r.SearchType}
                      </>
                    }
                    onClick={() => selectResult(r)}
                  />
                ))
              )}
              {!searching && searched && !results.length && (
                <WorkEmpty title="No indexed matches">
                  Try a symbol name or a shorter phrase, or choose another
                  context.
                </WorkEmpty>
              )}
              {!searched && (
                <div className="investigation-guide">
                  <p>Search functions, types and files across the index.</p>
                  <ol>
                    <li>Locate the implementation.</li>
                    <li>Inspect the indexed source.</li>
                    <li>Follow incoming or outgoing relationships.</li>
                    <li>Review potential impact before changing code.</li>
                  </ol>
                  <button
                    className="work-button"
                    onClick={() => setView("query")}
                  >
                    Explore with Cypher
                  </button>
                </div>
              )}
            </section>
            <section
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
                    <WorkBadge>Indexed evidence</WorkBadge>
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
          </div>
        </>
      )}
      {view === "query" && (
        <>
          <div className="query-lab-layout">
            <WorkSection
              title="A reproducible question"
              description="Run a read-only Cypher query. AI can draft it; you review and execute it."
            >
              <QueryBar
                key={currentScope}
                contextId={context}
                projectDir={projectDir}
                loading={queryLoading}
                setLoading={setQueryLoading}
                onQueryStart={() => {
                  const request = ++graphRequest.current;
                  return () => request === graphRequest.current;
                }}
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
                  setQueryRan(true);
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
          {queryRan && (
            <WorkSection
              title="Query result"
              actions={
                nodes.length ? (
                  <button
                    className="work-button"
                    onClick={() => setView("map")}
                  >
                    View map
                  </button>
                ) : undefined
              }
            >
              {tabular ? (
                <div className="query-table">
                  <TabularResults
                    columns={tabular.columns}
                    rows={tabular.rows}
                    onClose={() => setTabular(null)}
                  />
                </div>
              ) : nodes.length ? (
                nodes.map((n) => (
                  <RecordLink
                    key={n.id}
                    title={n.name || n.id}
                    meta={n.label + " · " + (n.file || "")}
                    onClick={() => choose(n)}
                  />
                ))
              ) : (
                <WorkEmpty title="No rows returned">
                  Refine the query or check the selected context.
                </WorkEmpty>
              )}
            </WorkSection>
          )}
        </>
      )}
      {view === "map" && (
        <div className="map-workspace">
          <WorkNotice title="A bounded view of indexed relationships">
            The map shows the current query result. A sample is limited and does
            not represent the complete repository.
          </WorkNotice>
          <button
            className="work-button mb-4"
            disabled={queryLoading}
            onClick={() => void sample()}
          >
            {queryLoading ? "Loading…" : "Load index sample"}
          </button>
          <InvestigationMap
            nodes={nodes}
            links={links}
            schema={schema}
            selectedNode={selected}
            handleNodeClick={choose}
            projectRoot={projectRoot}
            handleFileClick={(path, line) => {
              setView("investigate");
              choose({
                id: path,
                name: path.split("/").pop() || path,
                label: "File",
                type: "File",
                file: path,
                line,
              });
            }}
          />
        </div>
      )}
    </WorkPage>
  );
}

function InvestigationMap({
  nodes,
  links,
  schema,
  selectedNode,
  handleNodeClick,
  projectRoot,
  handleFileClick,
}: {
  nodes: GraphNode[];
  links: GraphEdge[];
  schema: SchemaResponse;
  selectedNode: GraphNode | null;
  handleNodeClick: (node: GraphNode | null) => void;
  projectRoot: string;
  handleFileClick: (path: string, line?: number) => void;
}) {
  const [hiddenLabels, setHiddenLabels] = useState<Set<string>>(
    () => new Set<string>(LS.get<string[]>("graphit_hidden_labels", [])),
  );
  const [hiddenEdgeTypes, setHiddenEdgeTypes] = useState<Set<string>>(
    () => new Set<string>(LS.get<string[]>("graphit_hidden_edges", [])),
  );
  const [hiddenClusters, setHiddenClusters] = useState<Set<string>>(
    () => new Set<string>(LS.get<string[]>("graphit_hidden_clusters", [])),
  );
  const [hiddenLangs, setHiddenLangs] = useState<Set<string>>(
    () => new Set<string>(LS.get<string[]>("graphit_hidden_langs", [])),
  );
  const [collapsedLangs, setCollapsedLangs] = useState<Set<string>>(
    () => new Set<string>(LS.get<string[]>("graphit_collapsed_langs", [])),
  );
  const [nodeColors, setNodeColors] = useState<Record<string, string>>(() =>
    LS.get<Record<string, string>>("graphit_node_colors", {}),
  );
  const [clusterColors, setClusterColors] = useState<Record<string, string>>(
    () => LS.get<Record<string, string>>("graphit_cluster_colors", {}),
  );
  const [langColors, setLangColors] = useState<Record<string, string>>(() =>
    LS.get<Record<string, string>>("graphit_lang_colors", {}),
  );
  const schemaNodes = schema.nodes || [],
    schemaEdges = schema.edges || [],
    schemaLangs = schema.langs || [];
  const canvasRef = useRef<GraphCanvasRef>(null);
  const [is3D, setIs3D] = useState(() => LS.get("graphit_is3D", false));
  const [physics, setPhysics] = useState(() =>
    LS.get("graphit_physics_" + (is3D ? "3d" : "2d"), {
      repulsion: 120,
      linkDistance: 50,
      gravity: is3D ? 0.1 : 0.3,
      edgeWidth: 1,
      labelDensity: 1.2,
    }),
  );
  const switchMode = () => {
    LS.set("graphit_physics_" + (is3D ? "3d" : "2d"), physics);
    const next = !is3D;
    setIs3D(next);
    LS.set("graphit_is3D", next);
    setPhysics(
      LS.get("graphit_physics_" + (next ? "3d" : "2d"), {
        repulsion: 120,
        linkDistance: 50,
        gravity: next ? 0.1 : 0.3,
        edgeWidth: 1,
        labelDensity: 1.2,
      }),
    );
  };
  useEffect(
    () => LS.set("graphit_physics_" + (is3D ? "3d" : "2d"), physics),
    [physics, is3D],
  );
  useEffect(() => {
    LS.set("graphit_node_colors", nodeColors);
  }, [nodeColors]);
  useEffect(() => {
    LS.set("graphit_hidden_labels", [...hiddenLabels]);
  }, [hiddenLabels]);
  useEffect(() => {
    LS.set("graphit_hidden_edges", [...hiddenEdgeTypes]);
  }, [hiddenEdgeTypes]);
  useEffect(() => {
    LS.set("graphit_hidden_clusters", [...hiddenClusters]);
  }, [hiddenClusters]);
  useEffect(() => {
    LS.set("graphit_hidden_langs", [...hiddenLangs]);
  }, [hiddenLangs]);
  useEffect(() => {
    LS.set("graphit_collapsed_langs", [...collapsedLangs]);
  }, [collapsedLangs]);
  useEffect(() => {
    LS.set("graphit_cluster_colors", clusterColors);
  }, [clusterColors]);
  useEffect(() => {
    LS.set("graphit_lang_colors", langColors);
  }, [langColors]);
  return (
    <div className="investigation-map">
      <div className="map-toolbar work-actions">
        <button
          className="work-button"
          onClick={() => canvasRef.current?.zoomBy?.(1.4)}
        >
          Zoom in
        </button>
        <button
          className="work-button"
          onClick={() => canvasRef.current?.zoomBy?.(1 / 1.4)}
        >
          Zoom out
        </button>
        <button
          className="work-button"
          onClick={() => canvasRef.current?.fitGraph?.()}
        >
          Fit result
        </button>
        <button className="work-button" onClick={switchMode}>
          {is3D ? "Use 2D" : "Use 3D"}
        </button>
        <span>
          {nodes.length} nodes · {links.length} relationships
        </span>
      </div>
      <div className="map-controls work-two-columns">
        <details className="work-disclosure">
          <summary>Map appearance & filters</summary>
          <div>
            <SchemaPanel
              nodes={schemaNodes}
              edges={schemaEdges}
              langs={schemaLangs}
              graphNodes={nodes}
              hiddenLabels={hiddenLabels}
              hiddenEdgeTypes={hiddenEdgeTypes}
              hiddenClusters={hiddenClusters}
              hiddenLangs={hiddenLangs}
              collapsedLangs={collapsedLangs}
              nodeColors={nodeColors}
              clusterColors={clusterColors}
              langColors={langColors}
              onToggleLabel={(l) =>
                setHiddenLabels((prev) => {
                  const next = new Set(prev);
                  if (next.has(l)) {
                    next.delete(l);
                  } else {
                    next.add(l);
                  }
                  return next;
                })
              }
              onToggleEdge={(t) =>
                setHiddenEdgeTypes((prev) => {
                  const next = new Set(prev);
                  if (next.has(t)) {
                    next.delete(t);
                  } else {
                    next.add(t);
                  }
                  return next;
                })
              }
              onToggleCluster={(c) =>
                setHiddenClusters((prev) => {
                  const next = new Set(prev);
                  if (next.has(c)) {
                    next.delete(c);
                  } else {
                    next.add(c);
                  }
                  return next;
                })
              }
              onToggleLang={(l) =>
                setHiddenLangs((prev) => {
                  const next = new Set(prev);
                  if (next.has(l)) {
                    next.delete(l);
                  } else {
                    next.add(l);
                  }
                  return next;
                })
              }
              onToggleLangCollapse={(l) =>
                setCollapsedLangs((prev) => {
                  const next = new Set(prev);
                  if (next.has(l)) {
                    next.delete(l);
                  } else {
                    next.add(l);
                  }
                  return next;
                })
              }
              onColorChange={(label, color) => {
                setNodeColors((prev) => ({ ...prev, [label]: color }));
              }}
              onClusterColorChange={(cluster, color) => {
                setClusterColors((prev) => ({ ...prev, [cluster]: color }));
              }}
              onLangColorChange={(lang, color) => {
                setLangColors((prev) => ({ ...prev, [lang]: color }));
              }}
            />
          </div>
        </details>
        <div>
          <details className="work-disclosure">
            <summary>Layout physics</summary>
            <div className="work-form">
              {[
                {
                  key: "repulsion",
                  label: "Repulsion",
                  min: 50,
                  max: 800,
                  step: 10,
                },
                {
                  key: "linkDistance",
                  label: "Link distance",
                  min: 20,
                  max: 300,
                  step: 5,
                },
                {
                  key: "gravity",
                  label: "Cluster pull",
                  min: 0.01,
                  max: 1,
                  step: 0.01,
                },
                {
                  key: "edgeWidth",
                  label: "Edge width",
                  min: 1,
                  max: 10,
                  step: 0.5,
                },
                {
                  key: "labelDensity",
                  label: "Label threshold",
                  min: 0.1,
                  max: 3,
                  step: 0.1,
                },
              ].map((p) => (
                <label className="work-field" key={p.key}>
                  <span>
                    {p.label} · {physics[p.key as keyof typeof physics]}
                  </span>
                  <input
                    type="range"
                    min={p.min}
                    max={p.max}
                    step={p.step}
                    value={physics[p.key as keyof typeof physics]}
                    onChange={(e) =>
                      setPhysics((s) => ({ ...s, [p.key]: +e.target.value }))
                    }
                  />
                </label>
              ))}
            </div>
          </details>
          <details className="work-disclosure">
            <summary>Files in this result</summary>
            <NodeTree
              nodes={nodes}
              projectRoot={projectRoot}
              selectedNodeId={
                nodes.some((n) => n.id === selectedNode?.id)
                  ? selectedNode!.id
                  : null
              }
              onNodeClick={handleNodeClick}
              onFileClick={handleFileClick}
            />
          </details>
        </div>
      </div>
      <div className="map-stage">
        <GraphCanvas
          ref={canvasRef}
          nodes={nodes}
          links={links}
          hiddenLabels={hiddenLabels}
          hiddenEdgeTypes={hiddenEdgeTypes}
          hiddenClusters={hiddenClusters}
          hiddenLangs={hiddenLangs}
          nodeColors={nodeColors}
          clusterColors={clusterColors}
          langColors={langColors}
          selectedNodeId={
            nodes.some((n) => n.id === selectedNode?.id)
              ? selectedNode!.id
              : null
          }
          onNodeClick={handleNodeClick}
          physics={physics}
          is3D={is3D}
        />
      </div>
    </div>
  );
}
