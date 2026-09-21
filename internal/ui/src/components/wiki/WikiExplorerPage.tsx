import { AgentExecution, appendProgress, type ExecutionOutcome } from "@/components/shared/AgentExecution";
import type { AgentProgress } from "@/api/agentStream";
import { RecordReferences } from "@/components/shared/RecordReferences";
import { StyledSelect } from "@/components/shared/StyledSelect";
import { usePageRefresh } from "@/components/layout/WorkspaceRefresh";
import { ModalPortal } from "@/components/shared/ModalPortal";
import React, {
  useState,
  useEffect,
  useCallback,
  useMemo,
  useRef,
} from "react";
import { useParams, useNavigate, useLocation } from "react-router-dom";
import {
  fetchModules,
  fetchPages,
  fetchPage,
  searchWiki,
  aiSearchWiki,
  type WikiModule,
  type WikiPageMeta,
  type WikiPageContent,
  type SearchResult,
  type AISearchResponse,
} from "@/api/wiki";
import { WikiMarkdown } from "./WikiMarkdown";
import { agentFeaturesEnabled, wikiLinkFriendlyName } from "@/lib/utils";
import { useAppStore } from "@/store/appStore";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import {
  WorkBadge,
  WorkPage,
  WorkHeader,
  WorkSection,
  WorkSearch,
  WorkTabs,
  WorkEmpty,
  WorkNotice,
  FactList,
  RecordLink,
} from "@/components/shared/EngineeringUI";
import { X } from "lucide-react";
function preprocessContent(raw: string, title: string, type?: string): string {
  const lines = raw.split("\n");
  let i = 0;
  if (lines[0]?.trim() === "---") {
    i++;
    while (i < lines.length && lines[i].trim() !== "---") i++;
    i++;
  }

  const hasContentHeader = lines.some((line) => line.trim() === "## Content");
  const isSpecialPage =
    type === "log" ||
    type === "index" ||
    type === "community" ||
    type === "god-node";

  if (isSpecialPage || !hasContentHeader) {
    while (i < lines.length && lines[i].trim() === "") i++;
    if (lines[i]?.trim() === "# " + title) i++;
    const out: string[] = [];
    for (; i < lines.length; i++) {
      const line = lines[i].trim();
      if (
        line === "---" &&
        i + 1 < lines.length &&
        lines[i + 1].trim().startsWith("*Navigate:")
      )
        break;
      if (line.startsWith("*Navigate:") && line.endsWith("*")) continue;
      out.push(lines[i]);
    }
    return out.join("\n");
  }

  const preambleRe =
    /^(#{1,2}\s|>\s|\*\*Source:\*\*|\*\*Type:\*\*|\*\*Confidence:\*\*|\*Provenance:|\*Navigate:|---$|$)/;
  while (i < lines.length) {
    const line = lines[i].trim();
    if (line === "## Content") {
      i++;
      break;
    }
    if (line === "## Cross-References") {
      i++;
      continue;
    }
    if (preambleRe.test(line)) {
      i++;
      continue;
    }

    if (/^-\s+\[\[/.test(line) || /^-\s+\[[^\]]*\]\([^)]*\)/.test(line)) {
      i++;
      continue;
    }
    break;
  }
  while (i < lines.length && lines[i].trim() === "") i++;
  const out: string[] = [];
  for (; i < lines.length; i++) {
    const line = lines[i].trim();
    if (
      line === "---" &&
      i + 1 < lines.length &&
      lines[i + 1].trim().startsWith("*Navigate:")
    )
      break;
    if (line.startsWith("*Navigate:") && line.endsWith("*")) continue;
    out.push(lines[i]);
  }
  return out.join("\n");
}

function ImageLightbox({ src, onClose }: { src: string; onClose: () => void }) {
  const [scale, setScale] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const dragging = useRef(false);
  const didDrag = useRef(false);
  const lastPos = useRef({ x: 0, y: 0 });

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    setScale((s) => Math.max(0.5, Math.min(5, s - e.deltaY * 0.002)));
  }, []);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;
    dragging.current = true;
    setIsDragging(true);
    didDrag.current = false;
    lastPos.current = { x: e.clientX, y: e.clientY };
    e.preventDefault();
  }, []);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!dragging.current) return;
    const dx = e.clientX - lastPos.current.x;
    const dy = e.clientY - lastPos.current.y;
    if (Math.abs(dx) > 2 || Math.abs(dy) > 2) didDrag.current = true;
    setPan((p) => ({ x: p.x + dx, y: p.y + dy }));
    lastPos.current = { x: e.clientX, y: e.clientY };
  }, []);

  const handleMouseUp = useCallback(() => {
    dragging.current = false;
    setIsDragging(false);
  }, []);

  const handleOverlayClick = useCallback(() => {
    if (!didDrag.current) onClose();
  }, [onClose]);

  return (
    <ModalPortal onClose={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Image preview"
        className="fixed inset-0 z-[9999] bg-black/85 backdrop-blur-md flex items-center justify-center animate-in fade-in duration-200"
        style={{ cursor: isDragging ? "grabbing" : "grab" }}
        onClick={handleOverlayClick}
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
      >
        <div className="absolute top-4 right-4 flex items-center gap-2 z-10 bg-black/40 backdrop-blur-md border border-white/10 p-1.5 rounded-xl">
          <button
            onClick={(e) => {
              e.stopPropagation();
              setScale((s) => Math.min(5, s + 0.25));
            }}
            className="w-8 h-8 rounded-lg bg-white/10 hover:bg-white/20 text-white font-bold text-sm transition-colors"
          >
            +
          </button>
          <span className="text-white/80 text-xs font-mono min-w-[5ch] text-center">
            {Math.round(scale * 100)}%
          </span>
          <button
            onClick={(e) => {
              e.stopPropagation();
              setScale((s) => Math.max(0.5, s - 0.25));
            }}
            className="w-8 h-8 rounded-lg bg-white/10 hover:bg-white/20 text-white font-bold text-sm transition-colors"
          >
            −
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              setScale(1);
              setPan({ x: 0, y: 0 });
            }}
            className="px-3 py-1.5 rounded-lg bg-white/10 hover:bg-white/20 text-white text-xs font-semibold transition-colors"
          >
            Reset
          </button>
          <div className="w-px h-5 bg-white/10 mx-1" />
          <button
            aria-label="Close image preview"
            onClick={(e) => {
              e.stopPropagation();
              onClose();
            }}
            className="w-8 h-8 rounded-lg bg-white/10 hover:bg-white/20 text-white flex items-center justify-center transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <img
          src={src}
          alt="Zoomed view"
          className="max-w-[92vw] max-h-[92vh] bg-white rounded-2xl shadow-2xl select-none pointer-events-none transition-transform duration-75"
          style={{
            transform: `translate(${pan.x}px, ${pan.y}px) scale(${scale})`,
          }}
          draggable={false}
        />
      </div>
    </ModalPortal>
  );
}

interface WikiExplorerProps {
  autoSelectProject?: boolean;
}
export default function WikiExplorerPage({
  autoSelectProject,
}: WikiExplorerProps = {}) {
  const { moduleId } = useParams<{ moduleId?: string }>(),
    navigate = useNavigate(),
    location = useLocation();
  const { activeProjectDir, activeAgent } = useAppStore();
  const [modules, setModules] = useState<WikiModule[]>([]),
    [module, setModule] = useState<WikiModule | null>(null);
  const [pages, setPages] = useState<WikiPageMeta[]>([]),
    [page, setPage] = useState<WikiPageContent | null>(null);
  const [view, setView] = useState("library"),
    [searchMode, setSearchMode] = useState("keyword"),
    [query, setQuery] = useState("");
  const [filter, setFilter] = useState(""),
    [type, setType] = useState("all"),
    [results, setResults] = useState<SearchResult[]>([]);
  const [progress, setProgress] = useState<AgentProgress[]>([]);
  const [outcome, setOutcome] = useState<ExecutionOutcome>("idle");
  const searchAbort = useRef<AbortController | null>(null);
  const [answer, setAnswer] = useState<AISearchResponse | null>(null),
    [error, setError] = useState("");
  const [loading, setLoading] = useState(false),
    [searching, setSearching] = useState(false),
    [loadingPage, setLoadingPage] = useState(false);
  const [raw, setRaw] = useState(false),
    [lightboxSrc, setLightboxSrc] = useState<string | null>(null);
  const [history, setHistory] = useState<
      Array<{ module: WikiModule; path: string }>
    >([]),
    [historyIndex, setHistoryIndex] = useState(-1);
  const readingTitle = useRef<HTMLHeadingElement>(null);
  const navigationRequest = useRef(0);
  useEffect(() => {
    if (view === "document" && page) {
      readingTitle.current?.focus({ preventScroll: true });
      readingTitle.current?.scrollIntoView?.({ block: "start" });
    }
  }, [view, page?.path]);
  const generation = useRef(0),
    pageRequest = useRef(0),
    searchRequest = useRef(0),
    moduleRequest = useRef(0);
  const cache = useRef<Record<string, WikiPageMeta[]>>({});
  const historyRef = useRef(historyIndex);
  historyRef.current = historyIndex;
  const pendingExternal = useRef(
    location.state as {
      aiResponse?: AISearchResponse;
      searchQuery?: string;
    } | null,
  );
  const base = location.pathname.startsWith("/knowledge")
    ? "/knowledge/explorer"
    : "/wiki/explorer";
  const modulesRef = useRef(modules);
  modulesRef.current = modules;
  const selectedRef = useRef(module);
  selectedRef.current = module;
  useEffect(() => {
    const handler = (e: Event) => setLightboxSrc((e as CustomEvent).detail);
    document.addEventListener("wiki-lightbox", handler);
    return () => document.removeEventListener("wiki-lightbox", handler);
  }, []);
  const loadModule = useCallback(async (m: WikiModule, reset = true) => {
    searchAbort.current?.abort();
    setProgress([]); setOutcome("idle");
    const id = ++moduleRequest.current;
    pageRequest.current++;
    searchRequest.current++;
    selectedRef.current = m;
    setModule(m);
    setPages([]);
    setLoading(true);
    setError("");
    setSearching(false);
    setLoadingPage(false);
    if (reset) {
      setPage(null);
      setView("library");
      setAnswer(null);
      setResults([]);
      setHistory([]);
      setHistoryIndex(-1);
      setFilter("");
      setType("all");
    }
    try {
      const ps = cache.current[m.id] ?? (await fetchPages(m.path));
      if (id !== moduleRequest.current) return false;
      cache.current[m.id] = ps || [];
      setPages(ps || []);
      return true;
    } catch (e) {
      if (id === moduleRequest.current) setError((e as Error).message);
      return false;
    } finally {
      if (id === moduleRequest.current) setLoading(false);
    }
  }, []);
  useEffect(() => {
    const id = ++generation.current;
    moduleRequest.current++;
    pageRequest.current++;
    searchRequest.current++;
    searchAbort.current?.abort(); setProgress([]); setOutcome("idle"); setSearching(false);
    cache.current = {};
    setModules([]);
    setModule(null);
    setPages([]);
    setPage(null);
    setAnswer(null);
    setResults([]);
    setHistory([]);
    setHistoryIndex(-1);
    setView("library");
    setLoading(true);
    setError("");
    fetchModules(activeProjectDir || undefined)
      .then(async (ms) => {
        if (id !== generation.current) return;
        const list = ms || [];
        setModules(list);
        const m =
          list.find((m) => m.context === moduleId || m.id === moduleId) ||
          (autoSelectProject
            ? list.find((m) => m.context === "project")
            : list[0]);
        if (m) await loadModule(m);
        if (id !== generation.current) return;
        const ext = pendingExternal.current;
        if (ext?.aiResponse) {
          setAnswer(ext.aiResponse);
          setQuery(ext.searchQuery || "");
          setSearchMode("ai");
          setView("results");
          pendingExternal.current = null;
          navigate(location.pathname, { replace: true, state: null });
        }
      })
      .catch((e) => {
        if (id === generation.current) setError((e as Error).message);
      })
      .finally(() => {
        if (id === generation.current) setLoading(false);
      });
    return () => {
      generation.current++;
      moduleRequest.current++;
      pageRequest.current++;
      searchRequest.current++; searchAbort.current?.abort();
    };
  }, [activeProjectDir, activeAgent, moduleId, autoSelectProject, loadModule]);
  const chooseModule = (m: WikiModule) => {
    void loadModule(m);
    navigate(
      m.context === "project"
        ? base
        : base + "/" + encodeURIComponent(m.context),
      { replace: true },
    );
  };
  const openPage = useCallback(
    async (
      m: WikiModule,
      path: string,
      addHistory = true,
      intent = ++navigationRequest.current,
    ) => {
      const epoch = generation.current;
      if (selectedRef.current?.id !== m.id && !(await loadModule(m, false)))
        return;
      if (
        epoch !== generation.current ||
        intent !== navigationRequest.current ||
        selectedRef.current?.id !== m.id
      )
        return;
      const id = ++pageRequest.current;
      setPage(null);
      setView("document");
      setRaw(false);
      setLoadingPage(true);
      setError("");
      if (addHistory) {
        setHistory((h) => [
          ...h.slice(0, historyRef.current + 1),
          { module: m, path },
        ]);
        setHistoryIndex((i) => i + 1);
      }
      try {
        const p = await fetchPage(m.path, path);
        if (id === pageRequest.current) setPage(p);
      } catch (e) {
        if (id === pageRequest.current) setError((e as Error).message);
      } finally {
        if (id === pageRequest.current) setLoadingPage(false);
      }
    },
    [loadModule],
  );
  const requestedPage = new URLSearchParams(location.search).get('page');
  const requestedPageKey = useRef('');
  useEffect(() => {
    if (!requestedPage || !module || loading) return;
    const key = activeProjectDir + ':' + module.id + ':' + requestedPage;
    if (requestedPageKey.current === key) return;
    requestedPageKey.current = key;
    void openPage(module, requestedPage);
  }, [requestedPage, module, loading, activeProjectDir, openPage]);
  useEffect(() => { if (!requestedPage) requestedPageKey.current = ''; }, [requestedPage]);
  const onLink = useCallback(
    async (target: string) => {
      const epoch = generation.current,
        intent = ++navigationRequest.current;
      const normalized = (s: string) =>
        s
          .toLowerCase()
          .replace(/\.md$/, "")
          .replace(/[^a-z0-9]/g, "");
      let targetText = target,
        ordered = [
          ...(selectedRef.current ? [selectedRef.current] : []),
          ...modulesRef.current.filter((m) => m.id !== selectedRef.current?.id),
        ];
      const prefixed = modulesRef.current.find(
        (m) =>
          target.startsWith(m.context + "/") || target.startsWith(m.id + "/"),
      );
      if (prefixed) {
        targetText = target.slice(
          (target.startsWith(prefixed.id + "/")
            ? prefixed.id
            : prefixed.context
          ).length + 1,
        );
        ordered = [prefixed];
      }
      for (const m of ordered) {
        try {
          const ps = cache.current[m.id] ?? (await fetchPages(m.path));
          if (
            epoch !== generation.current ||
            intent !== navigationRequest.current
          )
            return;
          cache.current[m.id] = ps || [];
          const exact = ps.find(
            (p) =>
              p.path === targetText ||
              normalized(p.path) === normalized(targetText) ||
              normalized(p.title) === normalized(targetText),
          );
          const found =
            exact ||
            ps.find(
              (p) =>
                normalized(p.path).includes(normalized(targetText)) ||
                (p.source &&
                  normalized(p.source).includes(normalized(targetText))),
            );
          if (found) {
            void openPage(m, found.path, true, intent);
            return;
          }
        } catch {
          /* try other known source */
        }
      }
      setError(
        "The linked document is not available in the current knowledge contexts: " +
          target,
      );
    },
    [openPage],
  );
  const search = async () => {
    if (!module || !query.trim()) return;
    const id = ++searchRequest.current;
    searchAbort.current?.abort();
    const controller = new AbortController(); searchAbort.current = controller;
    setProgress([]); setOutcome("idle");
    setSearching(true);
    setError("");
    setView("results");
    setResults([]);
    setAnswer(null);
    try {
      if (searchMode === "ai" && agentFeaturesEnabled()) {
        const r = await aiSearchWiki(
          module.path,
          query.trim(),
          activeProjectDir || undefined,
          { signal: controller.signal, onProgress: event => { if (id === searchRequest.current && !controller.signal.aborted) setProgress(items => appendProgress(items, event)); } },
        );
        if (id === searchRequest.current) {
          setAnswer(r); setOutcome(r.error ? "failed" : "completed");
          if (r.error) setError(r.error);
        }
      } else {
        const r = await searchWiki(module.path, query.trim());
        if (id === searchRequest.current) setResults(r || []);
      }
    } catch (e) {
      if (id === searchRequest.current && !controller.signal.aborted) { setError((e as Error).message); setOutcome("failed"); }
    } finally {
      if (id === searchRequest.current) setSearching(false);
    }
  };
  const refresh = async () => {
    const current = page, selectedModule = module,
      epoch = generation.current, intent = ++navigationRequest.current;
    const list = await fetchModules(activeProjectDir || undefined);
    if (epoch !== generation.current || intent !== navigationRequest.current) return;
    setModules(list || []);
    const next = list.find(m => m.id === selectedModule?.id)
      || (!selectedModule ? list.find(m => m.context === moduleId || m.id === moduleId)
        || (autoSelectProject ? list.find(m => m.context === "project") : list[0]) : undefined);
    if (!next) {
      moduleRequest.current++; pageRequest.current++; searchRequest.current++;
      selectedRef.current = null; cache.current = {};
      setAnswer(null); setResults([]); setHistory([]); setHistoryIndex(-1);
      setLoading(false); setLoadingPage(false); setSearching(false);
      setModule(null); setPage(null); setPages([]); setView("library");
      return;
    }
    delete cache.current[next.id];
    const accepted = await loadModule(next, !selectedModule);
    if (!accepted || epoch !== generation.current || intent !== navigationRequest.current || selectedRef.current?.id !== next.id) return;
    if (current && view === "document") await openPage(next, current.path, false, intent);
  };
  const filtered = pages.filter(
    (p) =>
      (type === "all" || p.type === type) &&
      (p.title + " " + p.path + " " + (p.tags || []).join(" "))
        .toLowerCase()
        .includes(filter.toLowerCase()),
  );
  const content = page
    ? preprocessContent(page.content, page.title, page.type)
    : "";
  const backlinks = page
    ? pages.filter(
        (p) =>
          p.path !== page.path &&
          (p.links || []).some(
            (l) =>
              l.replace(/\.md$/, "") === page.path.replace(/\.md$/, "") ||
              l === page.title,
          ),
      )
    : [];
  const goHistory = (index: number) => {
    const h = history[index];
    if (h) {
      setHistoryIndex(index);
      void openPage(h.module, h.path, false);
    }
  };
  usePageRefresh(refresh);
  return (
    <WorkPage className="knowledge-library">
      <WorkHeader
        title="Knowledge library"
        description="Find the system's intent, read the maintained guidance and follow the sources behind it."
        actions={
          <>
            <button
              className="work-button"
              onClick={() => navigate("/knowledge/contexts")}
            >
              Knowledge contexts
            </button>

          </>
        }
      />
      <details
        className="knowledge-query-disclosure"
        open={view !== "document"}
      >
        <summary>
          Search & context · {module?.label || "Knowledge library"}
        </summary>
        <div className="knowledge-query">
          {(!autoSelectProject || Boolean(moduleId)) && <label className="work-field">
            <span>Knowledge context</span>
            <StyledSelect
              value={module?.id || ""}
              onChange={(e) => {
                const m = modules.find((m) => m.id === e.target.value);
                if (m) chooseModule(m);
              }}
              aria-label="Knowledge context"
            >
              {!modules.length && <option value="">No contexts</option>}
              {modules.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label} · {m.context}
                </option>
              ))}
            </StyledSelect>
          </label>}
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void search();
            }}
          >
            <div className="work-field">
              <span>
                {searchMode === "ai"
                  ? "Ask this knowledge base"
                  : "Search this knowledge base"}
              </span>
              <div className="work-actions">
                <WorkSearch
                  label="Search knowledge"
                  value={query}
                  onChange={setQuery}
                  placeholder="Architecture, decisions or an engineering question"
                />
                <button
                  className="work-button primary"
                  disabled={!module || !query.trim() || searching}
                >
                  {searching
                    ? "Searching…"
                    : searchMode === "ai"
                      ? "Ask"
                      : "Find"}
                </button>
              </div>
            </div>
            {agentFeaturesEnabled() && (
              <div className="knowledge-search-mode">
                <label>
                  <input
                    type="radio"
                    checked={searchMode === "keyword"}
                    onChange={() => setSearchMode("keyword")}
                  />{" "}
                  Keywords
                </label>
                <label>
                  <input
                    type="radio"
                    checked={searchMode === "ai"}
                    onChange={() => setSearchMode("ai")}
                  />{" "}
                  Answer with sources
                </label>
              </div>
            )}
          </form>
        </div>
      </details>
      <WorkTabs
        value={view}
        onChange={setView}
        items={[
          ["library", "Library"],
          ["document", "Reading"],
          ["results", "Search results"],
        ]}
        label="Knowledge workspace"
      />
      {error && (
        <WorkNotice title="Knowledge request could not complete" tone="error">
          {error}
        </WorkNotice>
      )}
      {view === "library" &&
        (loading ? (
          <LoadingSpinner label="Loading library…" />
        ) : !modules.length ? (
          <WorkEmpty title="No indexed wikis found">
            Index documentation with graphit knowledge index docs/ or install a
            knowledge context.
          </WorkEmpty>
        ) : (
          <>
            <div className="work-toolbar">
              <WorkSearch
                label="Filter library pages"
                value={filter}
                onChange={setFilter}
              />
              <StyledSelect
                aria-label="Document type"
                value={type}
                onChange={(e) => setType(e.target.value)}
              >
                <option value="all">All document types</option>
                {Array.from(new Set(pages.map((p) => p.type)))
                  .sort()
                  .map((t) => (
                    <option key={t}>{t}</option>
                  ))}
              </StyledSelect>
              <small>
                {filtered.length} of {pages.length} documents
              </small>
            </div>
            <div className="knowledge-directory">
              <section className="work-table-wrap">
                <table className="work-table">
                  <thead>
                    <tr>
                      <th>Document</th>
                      <th>Kind</th>
                      <th>Source</th>
                      <th>Words</th>
                    </tr>
                  </thead>
                  <tbody>
                    {filtered.map((p) => (
                      <tr key={p.path}>
                        <td>
                          <button
                            className="record-title"
                            onClick={() =>
                              module && void openPage(module, p.path)
                            }
                          >
                            {p.title}
                          </button>
                          <small>{p.path}</small>
                        </td>
                        <td>{p.type}</td>
                        <td>
                          <span className="work-inline-code">
                            {p.source || "—"}
                          </span>
                        </td>
                        <td>{p.wordCount.toLocaleString()}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                {!filtered.length && (
                  <WorkEmpty title="No matching documents">
                    Change the filter or select another context.
                  </WorkEmpty>
                )}
              </section>
              <aside>
                <WorkSection title="About this collection">
                  <FactList
                    items={[
                      ["Context", module?.context],
                      ["Pages", pages.length],
                      [
                        "Entities",
                        pages.filter(
                          (p) =>
                            !["index", "log", "community", "god-node"].includes(
                              p.type,
                            ),
                        ).length,
                      ],
                      [
                        "Communities",
                        pages.filter((p) => p.type === "community").length,
                      ],
                      [
                        "Central nodes",
                        pages.filter((p) => p.type === "god-node").length,
                      ],
                      [
                        "Unique references",
                        new Set(pages.flatMap((p) => p.links || [])).size,
                      ],
                    ]}
                  />
                </WorkSection>
                <WorkSection title="Collection entry points">
                  {pages
                    .filter((p) => p.type === "index" || p.type === "log")
                    .map((p) => (
                      <RecordLink
                        key={p.path}
                        title={p.title}
                        meta={p.type}
                        onClick={() => module && void openPage(module, p.path)}
                      />
                    ))}
                </WorkSection>
              </aside>
            </div>
          </>
        ))}
      {view === "document" && (
        <>
          {loadingPage ? (
            <LoadingSpinner label="Loading document…" />
          ) : page ? (
            <article className="knowledge-reader">
              <header className="reader-header">
                <div className="work-actions">
                  <button
                    className="work-button"
                    disabled={historyIndex <= 0}
                    onClick={() => goHistory(historyIndex - 1)}
                  >
                    Previous document
                  </button>
                  <button
                    className="work-button"
                    disabled={historyIndex >= history.length - 1}
                    onClick={() => goHistory(historyIndex + 1)}
                  >
                    Next document
                  </button>
                  <WorkBadge>{page.type}</WorkBadge>
                </div>
                <h2 ref={readingTitle} tabIndex={-1}>
                  {page.title}
                </h2>
                <div className="work-actions">
                  <code>{page.path}</code>
                  <button
                    className="work-button"
                    aria-pressed={raw}
                    onClick={() => setRaw(!raw)}
                  >
                    {raw ? "Read document" : "View Markdown"}
                  </button>
                  <button
                    className="work-button"
                    onClick={() =>
                      navigator.clipboard
                        .writeText(page.content)
                        .catch(() => setError("Could not copy the document."))
                    }
                  >
                    Copy Markdown
                  </button>
                </div>
              </header>
              <div className="reader-body">
                <section className="reading-content">
                  {raw ? (
                    <pre className="work-code">{page.content}</pre>
                  ) : (
                    <><WikiMarkdown content={content} onLink={onLink} />
                    <RecordReferences kind="knowledge" id={page.path.replace(/\.md$/, "")} context={module?.context} /></>
                  )}
                </section>
                <aside className="reader-sources">
                  <WorkSection title="Provenance">
                    <FactList
                      items={[
                        ["Source", page.source || "Not recorded"],
                        ["Context", module?.context],
                        ["Words", page.wordCount],
                        [
                          "Indexed confidence",
                          page.confidence > 0
                            ? Math.round(page.confidence * 100) + "%"
                            : "Not recorded",
                        ],
                        ["Tags", (page.tags || []).join(", ")],
                      ]}
                    />
                  </WorkSection>
                  <WorkSection title="Referenced documents">
                    {[...new Set(page.links || [])].map((l) => (
                      <RecordLink
                        key={l}
                        title={wikiLinkFriendlyName(l)}
                        onClick={() => void onLink(l)}
                      />
                    ))}
                    {!page.links?.length && (
                      <p className="text-xs text-muted-foreground">
                        No references recorded.
                      </p>
                    )}
                  </WorkSection>
                  <WorkSection title="Referenced by">
                    {backlinks.map((p) => (
                      <RecordLink
                        key={p.path}
                        title={p.title}
                        onClick={() => module && void openPage(module, p.path)}
                      />
                    ))}
                    {!backlinks.length && (
                      <p className="text-xs text-muted-foreground">
                        No incoming links in this collection.
                      </p>
                    )}
                  </WorkSection>
                </aside>
              </div>
            </article>
          ) : (
            <WorkEmpty title="Open a document">
              Choose a page from the library or follow a source from the search
              results.
            </WorkEmpty>
          )}
        </>
      )}
      {view === "results" && searchMode === "ai" && <AgentExecution events={progress} running={searching} outcome={outcome} onCancel={() => { searchAbort.current?.abort(); searchRequest.current++; setSearching(false); setOutcome("cancelled"); setError("Generation stopped."); }} />}
      {view === "results" &&
        (searching ? (
          <LoadingSpinner
            label={
              searchMode === "ai"
                ? "Assembling an answer with sources…"
                : "Searching knowledge…"
            }
          />
        ) : answer ? (
          <div className="knowledge-answer">
            <section>
              <WorkNotice title="AI-assisted answer">
                Review the cited documents before applying this interpretation.
              </WorkNotice>
              <WikiMarkdown content={answer.answer} onLink={onLink} />
              {!answer.answer && !answer.results?.length && (
                <WorkEmpty title="No relevant pages found">
                  Try a more specific question.
                </WorkEmpty>
              )}
            </section>
            <aside>
              <WorkSection title="Source documents">
                {(answer.results || []).map((r) => (
                  <RecordLink
                    key={r.path}
                    title={r.title}
                    meta={r.relevance + " · " + r.score + "%"}
                    onClick={() => void onLink(r.path)}
                  />
                ))}
              </WorkSection>
            </aside>
          </div>
        ) : (
          <WorkSection
            title="Matching documents"
            description={
              results.length + " results in the selected knowledge context."
            }
          >
            {results.map((r) => (
              <article key={r.path} className="knowledge-search-result">
                <RecordLink title={r.title} meta={`${r.path} · score ${r.score}`} onClick={() => void onLink(r.path)} />
                <div className="markdown-preview"><WikiMarkdown content={r.snippet || ""} onLink={onLink} /></div>
              </article>
            ))}
            {!results.length && (
              <WorkEmpty title="Find an answer in your knowledge">
                Search by keywords or ask a question with sources.
              </WorkEmpty>
            )}
          </WorkSection>
        ))}
      {lightboxSrc && (
        <ImageLightbox src={lightboxSrc} onClose={() => setLightboxSrc(null)} />
      )}
    </WorkPage>
  );
}
