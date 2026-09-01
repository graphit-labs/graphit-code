import { useEffect, useState, useRef, useCallback } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { astApi, type GraphNode, type GraphEdge, type SchemaNodeStat, type SchemaEdgeStat, type SchemaLangGroup } from '@/api/ast'
import { showToast } from '@/hooks/useToast'
import { GraphCanvas, type GraphCanvasRef } from './GraphCanvas'
import { QueryBar, QueryBarCollapsed, QueryBarCollapseButton } from './QueryBar'
import { SchemaPanel } from './SchemaPanel'
import { NodeTree } from './NodeTree'
import { CodePanel } from './CodePanel'
import { TabularResults } from './TabularResults'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { labelColor } from '@/lib/utils'
import {
  ArrowLeft, Layers, FolderTree, ChevronLeft, ChevronRight,
  ZoomIn, ZoomOut, Maximize2, RotateCcw, Settings, X, Code2,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { useAppStore } from '@/store/appStore'

const LS = {
  get<T>(key: string, fallback: T): T {
    try { const v = localStorage.getItem(key); return v ? JSON.parse(v) : fallback } catch { /* parse error */ return fallback }
  },
  set(key: string, value: unknown) {
    try { localStorage.setItem(key, JSON.stringify(value)) } catch { /* storage full */ }
  },
}

type LeftTab = 'schema' | 'tree'

function useResizable(
  initial: number,
  min: number,
  max: number,
  direction: 'right' | 'left' = 'right',
  storageKey?: string,
) {
  const [size, setSize] = useState(() =>
    storageKey ? LS.get<number>(storageKey, initial) : initial
  )
  const dragging = useRef(false)
  const startX = useRef(0)
  const startSize = useRef(size)

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    dragging.current = true
    startX.current = e.clientX
    startSize.current = size
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'

    const onMove = (ev: MouseEvent) => {
      if (!dragging.current) return
      const delta = direction === 'right' ? ev.clientX - startX.current : startX.current - ev.clientX
      const next = Math.max(min, Math.min(max, startSize.current + delta))
      setSize(next)
    }
    const onUp = () => {
      dragging.current = false
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      if (storageKey) LS.set(storageKey, sizeRef.current)
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
    }
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
  }, [size, min, max, direction, storageKey])

  const sizeRef = useRef(size)
  // eslint-disable-next-line react-hooks/immutability
  useEffect(() => { sizeRef.current = size }, [size])

  return { size, onMouseDown }
}

export default function ExplorerPage() {
  const { contextId } = useParams<{ contextId: string }>()
  const navigate = useNavigate()
  const { activeProjectDir } = useAppStore()

  const [nodes, setNodes] = useState<GraphNode[]>([])
  const [links, setLinks] = useState<GraphEdge[]>([])
  const [schemaNodes, setSchemaNodes] = useState<SchemaNodeStat[]>([])
  const [schemaEdges, setSchemaEdges] = useState<SchemaEdgeStat[]>([])
  const [schemaLangs, setSchemaLangs] = useState<SchemaLangGroup[]>([])
  const [hiddenLabels, setHiddenLabels] = useState<Set<string>>(
    () => new Set<string>(LS.get<string[]>('graphit_hidden_labels', []))
  )
  const [hiddenEdgeTypes, setHiddenEdgeTypes] = useState<Set<string>>(
    () => new Set<string>(LS.get<string[]>('graphit_hidden_edges', []))
  )
  const [hiddenClusters, setHiddenClusters] = useState<Set<string>>(
    () => new Set<string>(LS.get<string[]>('graphit_hidden_clusters', []))
  )
  const [hiddenLangs, setHiddenLangs] = useState<Set<string>>(
    () => new Set<string>(LS.get<string[]>('graphit_hidden_langs', []))
  )
  const [collapsedLangs, setCollapsedLangs] = useState<Set<string>>(
    () => new Set<string>(LS.get<string[]>('graphit_collapsed_langs', []))
  )
  const [nodeColors, setNodeColors] = useState<Record<string, string>>(
    () => LS.get<Record<string, string>>('graphit_node_colors', {})
  )
  const [clusterColors, setClusterColors] = useState<Record<string, string>>(
    () => LS.get<Record<string, string>>('graphit_cluster_colors', {})
  )
  const [langColors, setLangColors] = useState<Record<string, string>>(
    () => LS.get<Record<string, string>>('graphit_lang_colors', {})
  )
  const [selectedNode, setSelectedNode] = useState<GraphNode | null>(null)
  const [sourceFile, setSourceFile] = useState<string | null>(null)
  const [sourceContent, setSourceContent] = useState('')
  const [sourceLine, setSourceLine] = useState<number | null>(null)
  const [tabularData, setTabularData] = useState<{ columns: string[]; rows: unknown[][] } | null>(null)
  const [leftTab, setLeftTab] = useState<LeftTab>('schema')
  const [leftCollapsed, setLeftCollapsed] = useState(
    () => window.matchMedia('(max-width: 767px)').matches || LS.get<boolean>('graphit_left_collapsed', false)
  )
  const [rightVisible, setRightVisible] = useState(false)
  const [physicsPanel, setPhysicsPanel] = useState(false)
  const [is3D, setIs3D] = useState(
    () => LS.get<boolean>('graphit_is3D', false)
  )
  const defaults2D = { repulsion: 120, linkDistance: 50, gravity: 0.3, edgeWidth: 1, labelDensity: 1.2 }
  const defaults3D = { repulsion: 120, linkDistance: 50, gravity: 0.1, edgeWidth: 1, labelDensity: 1.2 }

  const loadPhysics = (mode3D: boolean) => {
    const key = mode3D ? 'graphit_physics_3d' : 'graphit_physics_2d'
    const defaults = mode3D ? defaults3D : defaults2D
    const saved = LS.get(key, defaults)
    return { ...defaults, ...saved }
  }

  const [physics, setPhysics] = useState(() => loadPhysics(is3D))

  const [queryBarCollapsed, setQueryBarCollapsed] = useState(
    () => window.matchMedia('(max-width: 767px)').matches || LS.get<boolean>('graphit_querybar_collapsed', false)
  )
  const [loading, setLoading] = useState(true)
  const [queryLoading, setQueryLoading] = useState(false)
  const [projectName, setProjectName] = useState<string>('')
  const [projectRoot, setProjectRoot] = useState<string>('')

  const canvasRef = useRef<GraphCanvasRef>(null)

  const left = useResizable(272, 180, 480, 'right', 'graphit_left_width')
  const right = useResizable(380, 240, 600, 'left', 'graphit_right_width')

  const decodedContextId = contextId ? decodeURIComponent(contextId) : undefined

  useEffect(() => {
    const load = async () => {
      setLoading(true)
      try {
        const projDir = activeProjectDir || undefined
        const [graphData, schema] = await Promise.all([
          astApi.getGraph({ context: decodedContextId, project_dir: projDir }),
          astApi.getSchema(decodedContextId, projDir),
        ])
        setNodes(graphData.nodes ?? [])
        setLinks(graphData.links ?? [])
        setSchemaNodes(schema.nodes ?? [])
        setSchemaEdges(schema.edges ?? [])
        setSchemaLangs(schema.langs ?? [])
        
        const defaults: Record<string, string> = {}
        schema.node_labels?.forEach((l) => { defaults[l] = labelColor(l) })
        const saved = LS.get<Record<string, string>>('graphit_node_colors', {})
        setNodeColors({ ...defaults, ...saved })

        try {
          const ctxData = await astApi.getContexts(activeProjectDir || undefined)
          const name = ctxData.project_name || ''
          const root = ctxData.project_root || ''
          setProjectName(name)
          setProjectRoot(root)
          if (name) document.title = `Graphit AST — ${name}`
        } catch { /* non-critical context fetch */ }
      } catch {
        showToast('Failed to load graph', 'error')
      } finally {
        setLoading(false)
      }
    }
    load()
  }, [decodedContextId, activeProjectDir])

  const handleQueryResult = useCallback((result: unknown) => {
    const r = result as { nodes?: GraphNode[]; links?: GraphEdge[]; tabular?: { columns: string[]; rows: unknown[][] } }
    if (r.tabular && r.tabular.columns && r.tabular.rows) {
      setTabularData(r.tabular)
    } else {
      setTabularData(null)
      setNodes(r.nodes ?? [])
      setLinks(r.links ?? [])
    }
  }, [])

  const handleFileClick = useCallback(async (path: string, line?: number) => {
    setSourceFile(path)
    setSourceLine(line ?? null)
    setRightVisible(true)
    try {
      const data = await astApi.getFile(path, decodedContextId, activeProjectDir || undefined)
      setSourceContent(data.content ?? '')
    } catch {
      showToast('Failed to load file', 'error')
      setSourceContent('// Could not load file content.')
    }
  }, [decodedContextId, activeProjectDir])

  const handleFileClickRef = useRef(handleFileClick)

  const leftCollapsedRef = useRef(leftCollapsed)
  useEffect(() => {
    leftCollapsedRef.current = leftCollapsed
    handleFileClickRef.current = handleFileClick
  }, [leftCollapsed, handleFileClick])

  const handleNodeClick = useCallback((node: GraphNode | null) => {
    if (!node) { setSelectedNode(null); return }
    setSelectedNode(node)
    
    setLeftTab('tree')
    if (leftCollapsedRef.current) setLeftCollapsed(false)
    
    if (node.file) handleFileClickRef.current(node.file, node.line)
  }, [])

  const handleZoom = (dir: 1 | -1) => {
    canvasRef.current?.zoomBy?.(dir === 1 ? 1.4 : 1 / 1.4)
  }

  const handleFit = () => { canvasRef.current?.fitGraph?.() }

  const prevModeRef = useRef(is3D)
  useEffect(() => {
    if (prevModeRef.current !== is3D) {
      const oldKey = prevModeRef.current ? 'graphit_physics_3d' : 'graphit_physics_2d'
      LS.set(oldKey, physics)
      prevModeRef.current = is3D
      setPhysics(loadPhysics(is3D))
    } else {
      const key = is3D ? 'graphit_physics_3d' : 'graphit_physics_2d'
      LS.set(key, physics)
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [physics, is3D])
  useEffect(() => { LS.set('graphit_node_colors', nodeColors) }, [nodeColors])
  useEffect(() => { LS.set('graphit_hidden_labels', [...hiddenLabels]) }, [hiddenLabels])
  useEffect(() => { LS.set('graphit_hidden_edges', [...hiddenEdgeTypes]) }, [hiddenEdgeTypes])
  useEffect(() => { LS.set('graphit_hidden_clusters', [...hiddenClusters]) }, [hiddenClusters])
  useEffect(() => { LS.set('graphit_hidden_langs', [...hiddenLangs]) }, [hiddenLangs])
  useEffect(() => { LS.set('graphit_collapsed_langs', [...collapsedLangs]) }, [collapsedLangs])
  useEffect(() => { LS.set('graphit_cluster_colors', clusterColors) }, [clusterColors])
  useEffect(() => { LS.set('graphit_lang_colors', langColors) }, [langColors])
  useEffect(() => { LS.set('graphit_left_collapsed', leftCollapsed) }, [leftCollapsed])
  useEffect(() => { LS.set('graphit_querybar_collapsed', queryBarCollapsed) }, [queryBarCollapsed])
  useEffect(() => { LS.set('graphit_is3D', is3D) }, [is3D])
  
  const handleReset = async () => {
    setLoading(true)
    setSelectedNode(null)
    try {
      const data = await astApi.getGraph({ context: decodedContextId, project_dir: activeProjectDir || undefined })
      setNodes(data.nodes ?? [])
      setLinks(data.links ?? [])
      setTabularData(null)
    } catch {
      showToast('Failed to reset', 'error')
    } finally {
      setLoading(false)
    }
  }

  const displayName = projectName || (
    decodedContextId && decodedContextId !== '__project__'
      ? decodedContextId
      : (projectRoot ? projectRoot.split('/').pop() : 'Project')
  ) || 'Project'

  const toRelPath = (abs: string) => {
    if (projectRoot && abs.startsWith(projectRoot)) {
      return abs.slice(projectRoot.length).replace(/^\//, '')
    }
    return abs.split('/').slice(-3).join('/')
  }

  return (
    <div className="explorer-frame flex h-screen overflow-hidden bg-background/95">

      {}
      <div
        className={cn(
          'flex flex-col border-r border-border/40 bg-card/40 backdrop-blur-xl shrink-0 relative z-20 transition-all duration-300 ease-in-out',
          leftCollapsed ? 'w-14' : '',
        )}
        style={leftCollapsed ? undefined : { width: left.size }}
      >
        {}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border/40 gap-2 shrink-0">
          {!leftCollapsed ? (
            <button
              onClick={() => navigate('/ast/contexts')}
              className="flex items-center gap-1.5 text-xs font-semibold text-muted-foreground hover:text-foreground transition-colors group"
            >
              <ArrowLeft className="w-3.5 h-3.5 group-hover:-translate-x-0.5 transition-transform" />
              Contexts
            </button>
          ) : (
            <button
              onClick={() => navigate('/ast/contexts')}
              className="p-2 rounded-xl hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
              title="Back to Contexts"
            >
              <ArrowLeft className="w-4 h-4" />
            </button>
          )}
          <button
            onClick={() => setLeftCollapsed((v) => !v)}
            className="p-1.5 rounded-xl hover:bg-accent border border-transparent hover:border-border/30 text-muted-foreground hover:text-foreground transition-all"
          >
            {leftCollapsed ? <ChevronRight className="w-4 h-4" /> : <ChevronLeft className="w-4 h-4" />}
          </button>
        </div>

        {!leftCollapsed && (
          <>
            {}
            <div className="flex px-3 py-2 border-b border-border/30 bg-accent/10 gap-1 shrink-0">
              {[
                { id: 'schema' as LeftTab, icon: <Layers className="w-3.5 h-3.5" />, label: 'Schema' },
                { id: 'tree' as LeftTab, icon: <FolderTree className="w-3.5 h-3.5" />, label: 'Tree' },
              ].map((tab) => (
                <button
                  key={tab.id}
                  onClick={() => setLeftTab(tab.id)}
                  className={cn(
                    'flex items-center gap-2 px-3 py-1.5 text-xs font-semibold rounded-xl flex-1 justify-center transition-all duration-150',
                    leftTab === tab.id
                      ? 'bg-background text-foreground shadow-sm border border-border/30 font-bold'
                      : 'text-muted-foreground hover:text-foreground hover:bg-accent/40',
                  )}
                >
                  {tab.icon}
                  {tab.label}
                </button>
              ))}
            </div>

            {}
            <div className="flex-1 overflow-y-auto">
              {leftTab === 'schema' && (
                <div className="p-4">
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
                        const next = new Set(prev)
                        if (next.has(l)) { next.delete(l) } else { next.add(l) }
                        return next
                      })
                    }
                    onToggleEdge={(t) =>
                      setHiddenEdgeTypes((prev) => {
                        const next = new Set(prev)
                        if (next.has(t)) { next.delete(t) } else { next.add(t) }
                        return next
                      })
                    }
                    onToggleCluster={(c) =>
                      setHiddenClusters((prev) => {
                        const next = new Set(prev)
                        if (next.has(c)) { next.delete(c) } else { next.add(c) }
                        return next
                      })
                    }
                    onToggleLang={(l) =>
                      setHiddenLangs((prev) => {
                        const next = new Set(prev)
                        if (next.has(l)) { next.delete(l) } else { next.add(l) }
                        return next
                      })
                    }
                    onToggleLangCollapse={(l) =>
                      setCollapsedLangs((prev) => {
                        const next = new Set(prev)
                        if (next.has(l)) { next.delete(l) } else { next.add(l) }
                        return next
                      })
                    }
                    onColorChange={(label, color) => {
                      setNodeColors((prev) => ({ ...prev, [label]: color }))
                    }}
                    onClusterColorChange={(cluster, color) => {
                      setClusterColors((prev) => ({ ...prev, [cluster]: color }))
                    }}
                    onLangColorChange={(lang, color) => {
                      setLangColors((prev) => ({ ...prev, [lang]: color }))
                    }}
                  />
                </div>
              )}
              {leftTab === 'tree' && (
                <div className="p-2">
                  <NodeTree
                    nodes={nodes}
                    projectRoot={projectRoot}
                    selectedNodeId={selectedNode?.id ?? null}
                    onNodeClick={handleNodeClick}
                    onFileClick={handleFileClick}
                  />
                </div>
              )}
            </div>
          </>
        )}

        {}
        {!leftCollapsed && (
          <div
            className="absolute right-0 top-0 h-full w-1 cursor-col-resize hover:bg-primary/50 transition-colors z-20 group"
            onMouseDown={left.onMouseDown}
          >
            <div className="absolute right-0 top-1/2 -translate-y-1/2 w-1 h-8 rounded-full bg-border/40 group-hover:bg-primary/75 opacity-0 group-hover:opacity-100 transition-opacity" />
          </div>
        )}
      </div>

      {}
      <div className="flex-1 flex flex-col min-w-0 relative overflow-hidden">

        {}
        <div className="border-b border-border/40 bg-card/25 backdrop-blur-md shrink-0">
          {queryBarCollapsed ? (
            <QueryBarCollapsed onClick={() => setQueryBarCollapsed(false)} />
          ) : (
            <div className="flex items-start gap-3 px-6 py-4">
              <div className="flex-1 min-w-0">
                <QueryBar
                  contextId={decodedContextId}
                  projectDir={activeProjectDir || undefined}
                  onQueryResult={handleQueryResult}
                  loading={queryLoading}
                  setLoading={setQueryLoading}
                />
              </div>
              <QueryBarCollapseButton onClick={() => setQueryBarCollapsed(true)} />
            </div>
          )}
        </div>

        {}
        <div className="flex-1 relative overflow-hidden">
          {}
          <div className="absolute top-4 left-4 z-10 flex items-center gap-3 bg-background/70 backdrop-blur-md border border-border/40 px-4 py-2.5 rounded-2xl text-xs font-semibold pointer-events-none text-foreground/90 shadow-sm">
            <span className="flex items-center gap-2">
              <div className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
              {nodes.length} nodes
            </span>
            <span className="opacity-30">|</span>
            <span>{links.length} edges</span>
            {displayName && (
              <>
                <span className="opacity-30">|</span>
                <span className="font-bold text-primary max-w-[200px] truncate">{displayName}</span>
              </>
            )}
          </div>

          {}
          <div className="absolute top-4 right-4 z-10 flex flex-col gap-1 bg-background/70 backdrop-blur-md border border-border/40 rounded-2xl p-1.5 shadow-sm">
            <button
              onClick={() => handleZoom(1)}
              className="p-2.5 rounded-xl hover:bg-accent text-muted-foreground hover:text-foreground transition-all hover:scale-105"
              title="Zoom in"
            >
              <ZoomIn className="w-4 h-4" />
            </button>
            <button
              onClick={() => handleZoom(-1)}
              className="p-2.5 rounded-xl hover:bg-accent text-muted-foreground hover:text-foreground transition-all hover:scale-105"
              title="Zoom out"
            >
              <ZoomOut className="w-4 h-4" />
            </button>
            <div className="h-px bg-border/40 mx-2 my-1" />
            <button
              onClick={handleFit}
              className="p-2.5 rounded-xl hover:bg-accent text-muted-foreground hover:text-foreground transition-all hover:scale-105"
              title="Fit to screen"
            >
              <Maximize2 className="w-4 h-4" />
            </button>
            <button
              onClick={handleReset}
              className="p-2.5 rounded-xl hover:bg-accent text-muted-foreground hover:text-foreground transition-all hover:scale-105"
              title="Reset graph"
            >
              <RotateCcw className="w-4 h-4" />
            </button>
            <div className="h-px bg-border/40 mx-2 my-1" />
            <button
              onClick={() => setIs3D((v) => !v)}
              className={cn(
                "p-2.5 rounded-xl transition-all hover:scale-105",
                is3D
                  ? "bg-primary/15 text-primary border border-primary/25 hover:bg-primary/25"
                  : "hover:bg-accent text-muted-foreground hover:text-foreground border border-transparent"
              )}
              title={is3D ? "Switch to 2D Graph" : "Switch to 3D Graph"}
            >
              <Layers className="w-4 h-4" />
            </button>
            <button
              onClick={() => setPhysicsPanel((v) => !v)}
              className={cn(
                "p-2.5 rounded-xl transition-all hover:scale-105",
                physicsPanel
                  ? "bg-primary/15 text-primary border border-primary/25 hover:bg-primary/25"
                  : "hover:bg-accent text-muted-foreground hover:text-foreground border border-transparent"
              )}
              title="Physics settings"
            >
              <Settings className="w-4 h-4" />
            </button>
          </div>

          {}
          {selectedNode && (
            <div className="absolute bottom-6 left-6 z-20 bg-background/90 backdrop-blur-md border border-border/40 rounded-2xl p-5 w-80 shadow-2xl pointer-events-auto animate-in fade-in slide-in-from-bottom-4 duration-300">
              <div className="flex items-center justify-between mb-4 pb-3 border-b border-border/30">
                <div className="flex items-center gap-3 min-w-0">
                  <span className="w-3.5 h-3.5 rounded-full shrink-0 shadow-sm border border-black/10" style={{ background: labelColor(selectedNode.label) }} />
                  <span className="font-heading font-semibold text-sm text-foreground truncate" title={selectedNode.name || selectedNode.id}>
                    {selectedNode.name || selectedNode.id}
                  </span>
                </div>
                <button
                  onClick={() => setSelectedNode(null)}
                  className="p-1.5 rounded-xl hover:bg-accent/60 text-muted-foreground hover:text-foreground transition-colors"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>
              <div className="space-y-3">
                <div className="flex justify-between items-center bg-muted/40 px-3 py-2 rounded-xl border border-border/20">
                  <span className="text-[10px] uppercase tracking-widest text-muted-foreground font-bold">Label</span>
                  <span className="text-[11px] font-bold text-foreground bg-background px-2 py-0.5 rounded-lg border border-border/40 shadow-sm">{selectedNode.label}</span>
                </div>
                {selectedNode.type && selectedNode.type !== selectedNode.label && (
                  <div className="flex justify-between items-center px-1">
                    <span className="text-[11px] text-muted-foreground font-semibold">Kind</span>
                    <span className="text-[11px] font-medium text-foreground">{selectedNode.type}</span>
                  </div>
                )}
                {selectedNode.file && (
                  <div className="flex flex-col gap-1 px-1">
                    <span className="text-[11px] text-muted-foreground font-semibold">File Path</span>
                    <span className="text-[11px] font-mono text-foreground/80 bg-accent/20 px-2 py-1 rounded border border-border/20 truncate" title={selectedNode.file}>
                      {toRelPath(selectedNode.file)}
                    </span>
                  </div>
                )}
                {selectedNode.file && (
                  <button
                    onClick={() => handleFileClick(selectedNode.file!, selectedNode.line)}
                    className="flex items-center justify-center gap-2 text-xs font-bold bg-primary text-primary-foreground hover:bg-primary/90 shadow-md hover:shadow-lg transition-all rounded-xl py-2.5 mt-4 w-full group active:scale-[0.98]"
                  >
                    <Code2 className="w-4 h-4 group-hover:rotate-6 transition-transform" />
                    Open Source Code
                  </button>
                )}
              </div>
            </div>
          )}

          {}
          {physicsPanel && (
            <div className="absolute top-4 right-20 z-20 bg-background/90 backdrop-blur-md border border-border/40 rounded-2xl p-5 w-72 shadow-2xl animate-in fade-in slide-in-from-right-3 duration-200">
              <div className="flex items-center justify-between mb-4 pb-2 border-b border-border/30">
                <h4 className="text-xs font-bold uppercase tracking-wider text-foreground">Physics Configuration</h4>
                <button onClick={() => setPhysicsPanel(false)} className="p-1.5 rounded-xl hover:bg-accent/60 text-muted-foreground hover:text-foreground transition-colors">
                  <X className="w-4 h-4" />
                </button>
              </div>
              <div className="space-y-4">
                {[
                  { key: 'repulsion', label: 'Repulsion Strength', min: 50, max: 800, step: 10 },
                  { key: 'linkDistance', label: 'Link Distance', min: 20, max: 300, step: 5 },
                  { key: 'gravity', label: 'Cluster Pull (Gravity)', min: 0.01, max: 1.0, step: 0.01 },
                  { key: 'edgeWidth', label: 'Edge Thickness', min: 1, max: 10, step: 0.5 },
                  { key: 'labelDensity', label: 'Label Zoom Threshold', min: 0.1, max: 3.0, step: 0.1 },
                ].map(({ key, label, min, max, step }) => (
                  <div key={key} className="space-y-1.5">
                    <div className="flex items-center justify-between">
                      <label className="text-[11px] font-semibold text-muted-foreground">{label}</label>
                      <span className="text-[11px] font-mono font-bold text-primary bg-primary/10 px-1.5 py-0.5 rounded">{Number(physics[key as keyof typeof physics]).toFixed(1)}</span>
                    </div>
                    <input
                      type="range" min={min} max={max} step={step}
                      value={physics[key as keyof typeof physics]}
                      onChange={(e) => setPhysics((p) => ({ ...p, [key]: +e.target.value }))}
                      className="w-full h-1 bg-accent rounded-lg appearance-none cursor-pointer accent-primary"
                    />
                  </div>
                ))}
              </div>
            </div>
          )}

          {}
          {loading ? (
            <div className="flex items-center justify-center h-full">
              <LoadingSpinner size="lg" label="Loading graph..." />
            </div>
          ) : (
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
              selectedNodeId={selectedNode?.id ?? null}
              onNodeClick={handleNodeClick}
              physics={physics}
              is3D={is3D}
            />
          )}

          {}
          {tabularData && (
            <TabularResults
              columns={tabularData.columns}
              rows={tabularData.rows}
              onClose={() => setTabularData(null)}
            />
          )}
        </div>
      </div>

      {}
      {rightVisible && sourceFile && (
        <div
          className="shrink-0 border-l border-border/40 flex flex-col relative bg-card/40 backdrop-blur-xl z-20 animate-in slide-in-from-right duration-300"
          style={{ width: right.size }}
        >
          {}
          <div
            className="absolute left-0 top-0 h-full w-1 cursor-col-resize hover:bg-primary/50 transition-colors z-20 group"
            onMouseDown={right.onMouseDown}
          >
            <div className="absolute left-0 top-1/2 -translate-y-1/2 w-1 h-8 rounded-full bg-border/40 group-hover:bg-primary/75 opacity-0 group-hover:opacity-100 transition-opacity" />
          </div>

          <div className="flex-1 overflow-hidden p-3 bg-background/50">
            <CodePanel
              content={sourceContent}
              filename={sourceFile}
              highlightLine={sourceLine}
              onClose={() => { setRightVisible(false); setSourceFile(null); setSourceLine(null) }}
            />
          </div>
        </div>
      )}
    </div>
  )
}
