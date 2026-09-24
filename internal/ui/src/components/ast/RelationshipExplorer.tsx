import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, ArrowRight, Crosshair, FileCode2 } from "lucide-react";
import type { GraphEdge, GraphNode } from "@/api/ast";
import { StyledSelect } from "@/components/shared/StyledSelect";
import { WorkBadge, WorkEmpty, WorkSearch } from "@/components/shared/EngineeringUI";
import { groupGraph, groupOf, languageOf, clusterOf, observedGraph, type Grouping } from "./relationshipModel";
import "./relationship-explorer.css";
import { usePageRefresh } from "@/components/layout/WorkspaceRefresh";
import type { Neighborhood, NeighborhoodLoader } from "./neighborhood";

interface Props {
  nodes: GraphNode[];
  links: GraphEdge[];
  onInspect: (node: GraphNode) => void;
  loadNeighborhood?: NeighborhoodLoader;
  loading?: boolean;
  error?: boolean;
}
const entityIdentity = (n: GraphNode): string | null => {
  const identity = n.label === "File" || n.label === "Directory" ? n.properties?.path : n.properties?.uid;
  return typeof identity === "string" && identity ? JSON.stringify([n.label, identity]) : null;
};
const groupingLabels: Record<Grouping, string> = { directory: "Directory", file: "File", language: "Language", cluster: "Configured cluster" };

export function RelationshipExplorer({ nodes, links, onInspect, loadNeighborhood, loading = false, error = false }: Props) {
  const [grouping, setGrouping] = useState<Grouping>("directory");
  const [groupName, setGroupName] = useState<string | null>(null);
  const [focusId, setFocusId] = useState<string | null>(null);
  const [history, setHistory] = useState<GraphNode[]>([]);
  const [focusNode, setFocusNode] = useState<GraphNode | null>(null);
  const [neighborhood, setNeighborhood] = useState<Neighborhood | null>(null);
  const [neighborhoodLoading, setNeighborhoodLoading] = useState(false);
  const [neighborhoodError, setNeighborhoodError] = useState("");
  const [selectionError, setSelectionError] = useState("");
  const request = useRef<AbortController | null>(null);
  const fetchNeighborhood = useCallback(async (node: GraphNode, previous?: Neighborhood) => {
    if (!loadNeighborhood) return;
    request.current?.abort();
    const controller = new AbortController(); request.current = controller;
    setNeighborhoodLoading(true); setNeighborhoodError("");
    if (!previous) setNeighborhood(null);
    try {
      const data = await loadNeighborhood(node, previous, controller.signal);
      if (!controller.signal.aborted) setNeighborhood(data);
    } catch (e) {
      if (!controller.signal.aborted) setNeighborhoodError((e as Error).message);
    } finally {
      if (!controller.signal.aborted) setNeighborhoodLoading(false);
    }
  }, [loadNeighborhood]);
  useEffect(() => () => { request.current?.abort(); }, []);
  usePageRefresh(async () => { if (focusNode) await fetchNeighborhood(focusNode); });
  const reader = useRef<HTMLElement>(null);
  const [navigation, setNavigation] = useState(0);
  useLayoutEffect(() => { if (navigation && reader.current) { reader.current.focus({ preventScroll: true }); reader.current.scrollTop = 0; } }, [navigation]);
  const [filter, setFilter] = useState("");
  const [language, setLanguage] = useState("");
  const [entityType, setEntityType] = useState("");
  const [edgeType, setEdgeType] = useState("");
  const graph = useMemo(() => observedGraph(nodes, links), [nodes, links]);
  const visible = useMemo(() => {
    const selected = graph.nodes.filter(n => (!language || languageOf(n) === language) && (!entityType || n.label === entityType));
    const ids = new Set(selected.map(n => n.id));
    return { nodes: selected, links: graph.links.filter(e => ids.has(e.source) && ids.has(e.target) && (!edgeType || e.type === edgeType)) };
  }, [graph, language, entityType, edgeType]);
  const groups = useMemo(() => groupGraph(visible.nodes, visible.links, grouping), [visible, grouping]);
  const q = filter.trim().toLowerCase();
  const matched = groups.filter(g => !q || g.name.toLowerCase().includes(q) || g.members.some(n => (n.name + " " + n.file).toLowerCase().includes(q)));
  const active = matched.find(g => g.name === groupName) || matched[0];
  const focus = loadNeighborhood ? (neighborhood?.anchor || focusNode) : focusId ? visible.nodes.find(n => entityIdentity(n) === focusId) : null;
  const evidence = loadNeighborhood ? observedGraph(neighborhood?.nodes || [], neighborhood?.links || []) : { ...graph, links: visible.links };
  const incoming = focus ? evidence.links.filter(e => e.target === focus.id && e.source !== focus.id) : [];
  const outgoing = focus ? evidence.links.filter(e => e.source === focus.id && e.target !== focus.id) : [];
  const self = focus ? evidence.links.filter(e => e.source === focus.id && e.target === focus.id) : [];
  const pending = neighborhood?.partitions.filter(p => p.more || p.error) || [];
  const failed = neighborhood?.partitions.filter(p => p.error) || [];
  const select = (n: GraphNode | null) => {
    request.current?.abort();
    const identity = n ? entityIdentity(n) : null;
    if (n && !identity) {
      setNeighborhood(null); setNeighborhoodLoading(false); setNeighborhoodError("");
      setFocusNode(n); setFocusId(null);
      setSelectionError("Indexed identity unavailable; reindex this context before exploring the entity.");
      setNavigation(v => v + 1);
      return;
    }
    setSelectionError("");
    setNeighborhood(null); setNeighborhoodError(""); setNeighborhoodLoading(Boolean(n && loadNeighborhood));
    setFocusNode(n); setFocusId(identity); setNavigation(v => v + 1);
    if (n) void fetchNeighborhood(n);
  };
  const follow = (n: GraphNode) => {
    if (focus && entityIdentity(focus) !== entityIdentity(n) && entityIdentity(n)) setHistory(h => [...h, focus]);
    select(n);
  };
  const chooseGroup = (name: string) => { setGroupName(name); select(null); setHistory([]); };
  const clearFilters = () => { setFilter(""); setLanguage(""); setEntityType(""); setEdgeType(""); };
  function entity(n: GraphNode, prefix: string) {
    return <button key={prefix + n.id} className="relationship-entity" onClick={() => follow(n)}>
      <span className="relationship-entity-name"><span>{n.properties?.search_rank != null && <>#{String(n.properties.search_rank)} </>}{n.name || n.id}</span><ArrowRight size={14} aria-hidden="true" /></span>
      <span className="relationship-entity-location">{n.file || "No indexed source path"}{n.line ? `:${n.line}` : ""}</span>
      <span className="relationship-entity-meta">{n.label} <span>{languageOf(n)}</span></span>
      {Boolean(n.properties?.docstring) && <span className="relationship-entity-location">{String(n.properties?.docstring)}</span>}
    </button>;
  }
  function edges(items: GraphEdge[], side: "incoming" | "outgoing", prefix: string) {
    return items.length ? items.map(e => {
      const endpoint = (focus ? evidence : graph).byId.get(side === "incoming" ? e.source : e.target)!;
      const inner = (focus ? evidence : graph).byId.get(side === "incoming" ? e.target : e.source)!;
      return <div className="relationship-edge" key={e.uid || JSON.stringify([prefix,e.source,e.type,e.target])} data-relationship-uid={e.uid} data-native-relation-id={e.id}>
        <span className="relationship-edge-type" title={e.uid ? `Relationship UID: ${e.uid}` : undefined}>{e.type}</span>
        {entity(endpoint, prefix)}
        {!focus && <small>{side === "incoming" ? "To " : "From "}<button className="relationship-inline" onClick={() => follow(inner)}>{inner.name}</button></small>}
      </div>;
    }) : <p className="relationship-none">{selectionError ? "Relationship coverage is unavailable." : focus && loadNeighborhood ? (neighborhoodLoading ? "Loading " + side + " relationships…" : neighborhoodError || failed.length ? "Relationship coverage is incomplete." : "No " + side + " relationships found in the queried index.") : "No " + side + " relationships in this result."}</p>;
  }
  return <section className="relationship-explorer" aria-label="Relationship exploration">
    <div className="relationship-filters">
      <label className="work-field"><span>Organize by</span><StyledSelect value={grouping} onChange={e => { setGrouping(e.target.value as Grouping); setGroupName(null); }}>
        {Object.entries(groupingLabels).map(([value,label]) => <option key={value} value={value}>{label}</option>)}
      </StyledSelect></label>
      <label className="work-field"><span>Language</span><StyledSelect value={language} onChange={e => setLanguage(e.target.value)}>
        <option value="">All languages</option>{[...new Set(graph.nodes.map(languageOf))].sort().map(v => <option key={v}>{v}</option>)}
      </StyledSelect></label>
      <label className="work-field"><span>Entity type</span><StyledSelect value={entityType} onChange={e => setEntityType(e.target.value)}>
        <option value="">All entities</option>{[...new Set(graph.nodes.map(n => n.label))].sort().map(v => <option key={v}>{v}</option>)}
      </StyledSelect></label>
      <label className="work-field"><span>Relationship</span><StyledSelect value={edgeType} onChange={e => setEdgeType(e.target.value)}>
        <option value="">All relationships</option>{[...new Set(graph.links.map(e => e.type))].sort().map(v => <option key={v}>{v}</option>)}
      </StyledSelect></label>
      {(language || entityType || edgeType || filter) && <button className="work-button" onClick={clearFilters}>Clear filters</button>}
    </div>
    <div className="relationship-scope"><span><strong>{visible.nodes.length.toLocaleString()}</strong> entities · <strong>{visible.links.length.toLocaleString()}</strong> unique relationships in this view</span><span>Source → target · indexed evidence</span></div>
    {grouping === "cluster" && <p className="relationship-explanation">Clusters are configured path groups, not inferred communities or team ownership. Unassigned entities remain visible.</p>}
    {!graph.nodes.length ? (loading || error ? null : <WorkEmpty title="No graph entities in this result">Load an index sample or refine your search or query. Returned scalar values appear in Query rows in this workspace.</WorkEmpty>) : <div className="relationship-layout">
      <aside className="relationship-catalogue" aria-label="Result boundaries">
        <WorkSearch label="Find a boundary or entity" value={filter} onChange={setFilter} placeholder="Find a path or symbol" />
        <div className="relationship-catalogue-title"><h2>{groupingLabels[grouping]}</h2><span>{matched.length} groups</span></div>
        <div className="relationship-groups">{matched.map(g => <button key={g.name} className="relationship-group" aria-pressed={active?.name === g.name} onClick={() => chooseGroup(g.name)}>
          <strong>{g.name}</strong><span>{g.members.length} entities</span>
          <small><span>← {g.incoming.length} incoming</span><span>{g.outgoing.length} outgoing →</span></small>
        </button>)}</div>
        <p className="relationship-explanation">Groups describe this loaded result. Missing links do not prove independence.</p>
      </aside>
      <section ref={reader} tabIndex={-1} className="relationship-reader" aria-label="Relationship evidence">
        {selectionError && <p role="alert">{selectionError}</p>}
        {!active && !focus ? <WorkEmpty title="No matching entities" action={<button className="work-button" onClick={clearFilters}>Clear filters</button>}>Try another path, name or filter.</WorkEmpty> : focus ? <>
          <header className="relationship-reader-header"><div className="relationship-breadcrumb"><button className="work-button" onClick={() => { select(null); setHistory([]); }}>All boundaries</button>{history.length > 0 && <button className="work-button" onClick={() => { select(history[history.length-1]); setHistory(h => h.slice(0,-1)); }}><ArrowLeft size={14} />Back</button>}<span>Entity neighborhood</span></div>
            <div className="relationship-focus-heading"><div><WorkBadge>{focus.label}</WorkBadge><h2>{focus.name || focus.id}</h2><code>{focus.file || "No indexed source path"}{focus.line ? `:${focus.line}` : ""}</code></div><button className="work-button primary" onClick={() => onInspect(focus)}><FileCode2 size={16} />Inspect source & impact</button></div>
          </header>
          {loadNeighborhood && !selectionError && <div className="relationship-neighborhood-status" aria-live="polite">
            <p>{neighborhoodLoading ? "Loading indexed neighborhood…" : "Direct relationships from the selected index. Search results and catalogue remain unchanged."}</p>
            {neighborhoodError && <p role="alert">{neighborhoodError}</p>}
            {failed.length > 0 && <details><summary>{failed.length} relationship queries failed · partial evidence</summary><ul>{failed.map(p => <li key={[p.direction,p.type,p.target.label].join(":")}>{p.direction} · {p.type} · {p.target.label}: {p.error}</li>)}</ul></details>}
            {pending.some(p => p.more && !p.error) && <p>More relationships are available. Up to 100 neighbors per direction, relationship and entity type are loaded at a time.</p>}
            {!neighborhoodLoading && (neighborhoodError || pending.length > 0) && <button className="work-button" onClick={() => void fetchNeighborhood(focus, neighborhood || undefined)}>{neighborhoodError ? "Retry neighborhood" : failed.length ? "Retry / load remaining relationships" : "Load more relationships"}</button>}
          </div>}
          <div className="relationship-flow">
            <section><h3>Incoming <span>{incoming.length}</span></h3><p>Entities pointing to this one</p>{edges(incoming,"incoming","focus-in")}</section>
            <section className="relationship-anchor"><Crosshair size={22} aria-hidden="true" /><span>Selected entity</span><strong>{focus.name || focus.id}</strong><WorkBadge>{languageOf(focus)}</WorkBadge><dl><dt>UID</dt><dd><code>{String(focus.properties?.uid || focus.properties?.path || "Unavailable")}</code></dd><dt>Directory</dt><dd>{groupOf(focus,"directory")}</dd><dt>Configured cluster</dt><dd>{clusterOf(focus)}</dd></dl>{self.length > 0 && <p>Self relationships: {self.map(e=>e.type).join(", ")}</p>}</section>
            <section><h3>Outgoing <span>{outgoing.length}</span></h3><p>Entities this one points to</p>{edges(outgoing,"outgoing","focus-out")}</section>
          </div>
        </> : active && <>
          <header className="relationship-reader-header"><span>{groupingLabels[grouping]} boundary</span><h2>{active.name}</h2><p>{active.members.length} entities · {active.internal.length} internal relationships · {active.incoming.length + active.outgoing.length} crossing this boundary</p></header>
          <div className="relationship-boundary-content"><section className="relationship-members"><h3>Choose an entity</h3><p>Follow its relationships, then inspect the implementation.</p>{active.members.filter(n => !q || active.name.toLowerCase().includes(q) || (n.name + " " + n.file).toLowerCase().includes(q)).map(n => entity(n,"member"))}</section>
          <div className="relationship-crossings"><section><h3>Incoming <span>{active.incoming.length}</span></h3>{edges(active.incoming,"incoming","group-in")}</section><section><h3>Outgoing <span>{active.outgoing.length}</span></h3>{edges(active.outgoing,"outgoing","group-out")}</section></div></div>
        </>}
      </section>
    </div>}
    <footer className="relationship-footnote">This is a bounded result, not a complete architecture or runtime impact analysis. Select an entity to query its direct neighborhood independently. Indexed relationships do not prove runtime impact.</footer>
  </section>;
}
