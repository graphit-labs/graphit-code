import { useMemo } from 'react'
import { cn, labelColor, langKey } from '@/lib/utils'
import type { SchemaNodeStat, SchemaEdgeStat, SchemaLangGroup, GraphNode } from '@/api/ast'
import { ChevronDown, ChevronRight, Eye, EyeOff, Minus, Tags } from 'lucide-react'

const CLUSTER_DEFAULT_COLORS = [
  '#8b5cf6', '#06b6d4', '#f59e0b', '#ec4899', '#10b981',
  '#6366f1', '#14b8a6', '#f97316', '#a855f7', '#22d3ee',
]

interface SchemaPanelProps {
  nodes: SchemaNodeStat[]
  edges: SchemaEdgeStat[]
  langs: SchemaLangGroup[]
  graphNodes: GraphNode[]
  hiddenLabels: Set<string>
  hiddenEdgeTypes: Set<string>
  hiddenClusters: Set<string>
  hiddenLangs: Set<string>
  collapsedLangs: Set<string>
  nodeColors: Record<string, string>
  clusterColors: Record<string, string>
  langColors: Record<string, string>
  onToggleLabel: (label: string) => void
  onToggleEdge: (type: string) => void
  onToggleCluster: (cluster: string) => void
  onToggleLang: (lang: string) => void
  onToggleLangCollapse: (lang: string) => void
  onColorChange: (label: string, color: string) => void
  onClusterColorChange: (cluster: string, color: string) => void
  onLangColorChange: (lang: string, color: string) => void
}

interface LabelRowProps {
  label: string
  count: number
  color: string
  hidden: boolean
  onToggle: () => void
  onColorChange: (color: string) => void
}

function LabelRow({ label, count, color, hidden, onToggle, onColorChange }: LabelRowProps) {
  return (
    <div className="flex items-center gap-2.5 px-2 py-1.5 rounded-lg hover:bg-accent/30 border border-transparent hover:border-border/20 transition-all duration-150 group">
      {}
      <div className="relative w-4.5 h-4.5 rounded-full overflow-hidden border border-border/50 shadow-inner flex items-center justify-center shrink-0 cursor-pointer">
        <input
          type="color"
          value={color}
          onChange={(e) => onColorChange(e.target.value)}
          className="absolute inset-0 opacity-0 w-full h-full cursor-pointer"
          title="Change color"
        />
        <div className="w-2.5 h-2.5 rounded-full" style={{ backgroundColor: color }} />
      </div>
      {}
      <button
        onClick={onToggle}
        className="shrink-0 text-muted-foreground/60 hover:text-foreground transition-colors"
        title={hidden ? 'Show' : 'Hide'}
      >
        {hidden ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
      </button>
      {}
      <span
        className={cn(
          'flex-1 px-2.5 py-0.5 rounded-full text-xs font-semibold truncate transition-all duration-300',
          hidden ? 'opacity-30 line-through bg-muted text-muted-foreground' : 'shadow-sm'
        )}
        style={hidden ? {} : { background: `${color}18`, color }}
      >
        {label}
      </span>
      <span className="text-[10px] font-mono font-semibold text-muted-foreground/75 bg-accent/20 px-1.5 py-0.5 rounded-md border border-border/30 shrink-0">{count}</span>
    </div>
  )
}

export function SchemaPanel({
  nodes,
  edges,
  langs,
  graphNodes,
  hiddenLabels,
  hiddenEdgeTypes,
  hiddenClusters,
  hiddenLangs,
  collapsedLangs,
  nodeColors,
  clusterColors,
  langColors,
  onToggleLabel,
  onToggleEdge,
  onToggleCluster,
  onToggleLang,
  onToggleLangCollapse,
  onColorChange,
  onClusterColorChange,
  onLangColorChange,
}: SchemaPanelProps) {
  const clusterStats = useMemo(() => {
    const map = new Map<string, number>()
    for (const node of graphNodes) {
      const cluster = (node.properties?.cluster as string) ?? ''
      if (cluster) {
        map.set(cluster, (map.get(cluster) ?? 0) + 1)
      }
    }
    return Array.from(map.entries())
      .map(([name, count]) => ({ name, count }))
      .sort((a, b) => b.count - a.count)
  }, [graphNodes])

  const grouped = langs.length > 0

  return (
    <div className="flex flex-col gap-6 text-sm">
      {}
      <div className="space-y-3">
        <h4 className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground/60 border-b border-border/40 pb-1 flex items-center gap-1.5">
          <Tags className="w-3 h-3 text-muted-foreground/80" />
          {grouped ? 'Node Labels by Language' : 'Node Labels'}
        </h4>

        {grouped ? (
          <div className="flex flex-col gap-2">
            {langs.map((group, i) => {
              const key = langKey(group.lang)
              const named = group.lang !== ''
              const collapsed = collapsedLangs.has(key)
              const langHidden = hiddenLangs.has(key)
              const langDefault = CLUSTER_DEFAULT_COLORS[i % CLUSTER_DEFAULT_COLORS.length]
              const langColorValue = langColors[key] ?? langDefault
              return (
                <div key={key} className="rounded-lg border border-border/30 bg-accent/10 overflow-hidden">
                  {}
                  <div className="flex items-center gap-2 px-2 py-1.5">
                    <button
                      onClick={() => onToggleLangCollapse(key)}
                      className="shrink-0 text-muted-foreground/60 hover:text-foreground transition-colors"
                      title={collapsed ? 'Expand' : 'Collapse'}
                      aria-expanded={!collapsed}
                    >
                      {collapsed ? <ChevronRight className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
                    </button>
                    {named ? (
                      <div className="relative w-4.5 h-4.5 rounded-full overflow-hidden border border-border/50 shadow-inner flex items-center justify-center shrink-0 cursor-pointer">
                        <input
                          type="color"
                          value={langColorValue}
                          onChange={(e) => onLangColorChange(key, e.target.value)}
                          className="absolute inset-0 opacity-0 w-full h-full cursor-pointer"
                          title="Change language color"
                        />
                        <div className="w-2.5 h-2.5 rounded-full" style={{ backgroundColor: langColorValue }} />
                      </div>
                    ) : (
                      <div className="w-4.5 h-4.5 flex items-center justify-center shrink-0">
                        <div className="w-2.5 h-2.5 rounded-full bg-muted-foreground/30" />
                      </div>
                    )}
                    <button
                      onClick={() => onToggleLang(key)}
                      className="shrink-0 text-muted-foreground/60 hover:text-foreground transition-colors"
                      title={langHidden ? 'Show language' : 'Hide language'}
                    >
                      {langHidden ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
                    </button>
                    <span
                      className={cn(
                        'flex-1 text-xs font-bold truncate transition-all duration-300',
                        langHidden ? 'opacity-30 line-through text-muted-foreground' : 'text-foreground',
                        !named && 'italic font-semibold text-muted-foreground',
                      )}
                    >
                      {key}
                    </span>
                    <span className="text-[10px] font-mono font-semibold text-muted-foreground/75 bg-accent/30 px-1.5 py-0.5 rounded-md border border-border/30 shrink-0">{group.count}</span>
                  </div>
                  {}
                  {!collapsed && (
                    <div className="flex flex-col gap-1 pl-3 pr-1 pb-1.5 border-l-2 border-border/30 ml-4">
                      {group.labels.map((n) => (
                        <LabelRow
                          key={`${key}:${n.label}`}
                          label={n.label}
                          count={n.count}
                          color={nodeColors[n.label] ?? labelColor(n.label)}
                          hidden={hiddenLabels.has(n.label)}
                          onToggle={() => onToggleLabel(n.label)}
                          onColorChange={(c) => onColorChange(n.label, c)}
                        />
                      ))}
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        ) : (
          <div className="flex flex-col gap-1.5">
            {nodes.map((n) => (
              <LabelRow
                key={n.label}
                label={n.label}
                count={n.count}
                color={nodeColors[n.label] ?? labelColor(n.label)}
                hidden={hiddenLabels.has(n.label)}
                onToggle={() => onToggleLabel(n.label)}
                onColorChange={(c) => onColorChange(n.label, c)}
              />
            ))}
          </div>
        )}
      </div>

      {}
      {edges.length > 0 && (
        <div className="space-y-3">
          <h4 className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground/60 border-b border-border/40 pb-1">
            Edge Types
          </h4>
          <div className="flex flex-col gap-1.5">
            {edges.map((e) => {
              const hidden = hiddenEdgeTypes.has(e.type)
              return (
                <div key={e.type} className="flex items-center gap-2.5 px-2 py-1.5 rounded-lg hover:bg-accent/30 border border-transparent hover:border-border/20 transition-all duration-150 group">
                  <button
                    onClick={() => onToggleEdge(e.type)}
                    className="shrink-0 text-muted-foreground/60 hover:text-foreground transition-colors"
                    title={hidden ? 'Show relationship' : 'Hide relationship'}
                  >
                    {hidden ? <EyeOff className="w-3.5 h-3.5" /> : <Minus className="w-3.5 h-3.5" />}
                  </button>
                  <span
                    className={cn(
                      'flex-1 px-2 py-0.5 rounded text-[11px] font-mono font-medium bg-muted/60 text-muted-foreground border border-border/30 truncate transition-opacity',
                      hidden && 'opacity-30 line-through',
                    )}
                  >
                    {e.type}
                  </span>
                  <span className="text-[10px] font-mono font-semibold text-muted-foreground/75 bg-accent/20 px-1.5 py-0.5 rounded-md border border-border/30 shrink-0">{e.count}</span>
                </div>
              )
            })}
          </div>
        </div>
      )}

      {}
      {clusterStats.length > 0 && (
        <div className="space-y-3">
          <h4 className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground/60 border-b border-border/40 pb-1 flex items-center gap-1.5">
            <Tags className="w-3 h-3 text-muted-foreground/80" />
            Clusters
          </h4>
          <div className="flex flex-col gap-1.5">
            {clusterStats.map((c, i) => {
              const hidden = hiddenClusters.has(c.name)
              const defaultColor = CLUSTER_DEFAULT_COLORS[i % CLUSTER_DEFAULT_COLORS.length]
              const color = clusterColors[c.name] ?? defaultColor
              return (
                <div key={c.name} className="flex items-center gap-2.5 px-2 py-1.5 rounded-lg hover:bg-accent/30 border border-transparent hover:border-border/20 transition-all duration-150 group">
                  {}
                  <div className="relative w-4.5 h-4.5 rounded-full overflow-hidden border border-border/50 shadow-inner flex items-center justify-center shrink-0 cursor-pointer">
                    <input
                      type="color"
                      value={color}
                      onChange={(e) => onClusterColorChange(c.name, e.target.value)}
                      className="absolute inset-0 opacity-0 w-full h-full cursor-pointer"
                      title="Change cluster color"
                    />
                    <div className="w-2.5 h-2.5 rounded-full" style={{ backgroundColor: color }} />
                  </div>
                  {}
                  <button
                    onClick={() => onToggleCluster(c.name)}
                    className="shrink-0 text-muted-foreground/60 hover:text-foreground transition-colors"
                    title={hidden ? 'Show cluster' : 'Hide cluster'}
                  >
                    {hidden ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
                  </button>
                  {}
                  <span
                    className={cn(
                      'flex-1 px-2.5 py-0.5 rounded-full text-xs font-semibold truncate transition-all duration-300',
                      hidden ? 'opacity-30 line-through bg-muted text-muted-foreground' : 'shadow-sm'
                    )}
                    style={hidden ? {} : { background: `${color}18`, color }}
                  >
                    {c.name}
                  </span>
                  <span className="text-[10px] font-mono font-semibold text-muted-foreground/75 bg-accent/20 px-1.5 py-0.5 rounded-md border border-border/30 shrink-0">{c.count}</span>
                </div>
              )
            })}
          </div>
        </div>
      )}
    </div>
  )
}
