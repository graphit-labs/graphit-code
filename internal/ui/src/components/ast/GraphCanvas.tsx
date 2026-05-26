import { useEffect, useRef, useCallback, useMemo, forwardRef } from 'react'
import type { GraphNode, GraphEdge } from '@/api/ast'
import { labelColor } from '@/lib/utils'

const CLUSTER_DEFAULT_COLORS = [
  '#8b5cf6', '#06b6d4', '#f59e0b', '#ec4899', '#10b981',
  '#6366f1', '#14b8a6', '#f97316', '#a855f7', '#22d3ee',
]

interface GraphCanvasProps {
  nodes: GraphNode[]
  links: GraphEdge[]
  hiddenLabels: Set<string>
  hiddenEdgeTypes: Set<string>
  hiddenClusters: Set<string>
  hiddenLangs: Set<string>
  nodeColors: Record<string, string>
  clusterColors: Record<string, string>
  langColors: Record<string, string>
  selectedNodeId: string | null
  onNodeClick: (node: GraphNode | null) => void
  physics: { repulsion: number; linkDistance: number; gravity: number; edgeWidth: number; labelDensity: number }
  is3D: boolean
}

function clusterBounds(clusterNodes: { x?: number; y?: number }[]): { cx: number; cy: number; r: number } | null {
  if (clusterNodes.length === 0) return null
  let cx = 0, cy = 0
  let count = 0
  for (const n of clusterNodes) {
    if (n.x != null && n.y != null) {
      cx += n.x
      cy += n.y
      count++
    }
  }
  if (count === 0) return null
  cx /= count
  cy /= count
  let maxR = 0
  for (const n of clusterNodes) {
    if (n.x != null && n.y != null) {
      const d = Math.sqrt((n.x - cx) ** 2 + (n.y - cy) ** 2)
      if (d > maxR) maxR = d
    }
  }
  return { cx, cy, r: maxR + 30 }
}

export type GraphCanvasRef = HTMLDivElement & {
  fitGraph?: () => void
  zoomBy?: (factor: number) => void
}

function baseRadius(n: GraphNode): number {
  if (n.label === 'Package' || n.label === 'Module') return 12
  if (n.label === 'File') return 8
  return 6
}

function degreeRadius(n: GraphNode, degree: number, maxDegree: number): number {
  const base = baseRadius(n)
  if (maxDegree <= 0) return base
  const scale = 1 + (degree / maxDegree) * 2.2
  return base * scale
}

function getNeighbourIds(selectedId: string | null, links: { source: any; target: any }[]): Set<string> | null {
  if (!selectedId) return null
  const ids = new Set<string>([selectedId])
  for (const l of links) {
    const src = typeof l.source === 'object' ? l.source?.id : l.source
    const tgt = typeof l.target === 'object' ? l.target?.id : l.target
    if (src === selectedId || tgt === selectedId) {
      if (src) ids.add(src)
      if (tgt) ids.add(tgt)
    }
  }
  return ids
}

function linkNodeId(endpoint: any): string {
  return typeof endpoint === 'object' ? endpoint?.id : endpoint
}

function getBgColor(): string {
  const raw = getComputedStyle(document.documentElement).getPropertyValue('--background').trim()
  if (raw && !raw.startsWith('#') && !raw.startsWith('rgb')) {
    const [h, s, l] = raw.split(' ')
    return `hsl(${h}, ${s}, ${l})`
  }
  return document.documentElement.classList.contains('dark') ? '#0f1117' : '#ffffff'
}

export const GraphCanvas = forwardRef<GraphCanvasRef, GraphCanvasProps>(function GraphCanvas(
  { nodes, links, hiddenLabels, hiddenEdgeTypes, hiddenClusters, hiddenLangs, nodeColors, clusterColors, langColors, selectedNodeId, onNodeClick, physics, is3D },
  ref,
) {
  const localRef = useRef<HTMLDivElement>(null)
  const containerRef = (ref as React.RefObject<GraphCanvasRef | null>) ?? localRef
  const graphRef = useRef<any>(null)

  const selectedIdRef = useRef<string | null>(selectedNodeId)
  selectedIdRef.current = selectedNodeId

  const physicsRef = useRef(physics)
  physicsRef.current = physics

  const clusterColorsRef = useRef(clusterColors)
  clusterColorsRef.current = clusterColors
  const langColorsRef = useRef(langColors)
  langColorsRef.current = langColors
  const nodeColorsRef = useRef(nodeColors)
  nodeColorsRef.current = nodeColors

  const onNodeClickRef = useRef(onNodeClick)
  onNodeClickRef.current = onNodeClick

  console.log("GraphCanvas render!"); const visibleNodes = useMemo(() => nodes.filter((n) => {
    if (hiddenLabels.has(n.label)) return false
    if (hiddenClusters.size > 0) {
      const cluster = (n.properties?.cluster as string) ?? ''
      if (cluster && hiddenClusters.has(cluster)) return false
    }
    if (hiddenLangs.size > 0) {
      const lang = (n.properties?.lang as string) ?? ''
      if (lang && hiddenLangs.has(lang)) return false
    }
    return true
  }), [nodes, hiddenLabels, hiddenClusters, hiddenLangs])

  const visibleLinks = useMemo(() => {
    const nodeIdSet = new Set(visibleNodes.map((n) => n.id))
    return links.filter(
      (l) =>
        !hiddenEdgeTypes.has(l.type) &&
        nodeIdSet.has(l.source as string) &&
        nodeIdSet.has(l.target as string),
    )
  }, [links, visibleNodes, hiddenEdgeTypes])
  const visibleLinksRef = useRef(visibleLinks)
  visibleLinksRef.current = visibleLinks
  const visibleNodesRef = useRef(visibleNodes)
  visibleNodesRef.current = visibleNodes

  const neighbourhoodRef = useRef<Set<string> | null>(null)
  useEffect(() => {
    neighbourhoodRef.current = getNeighbourIds(selectedNodeId, visibleLinks)
  }, [selectedNodeId, visibleLinks])

  const { degreeMap, maxDegree } = useMemo(() => {
    const map = new Map<string, number>()
    for (const l of visibleLinks) {
      const src = typeof l.source === 'string' ? l.source : (l.source as any)?.id
      const tgt = typeof l.target === 'string' ? l.target : (l.target as any)?.id
      if (src) map.set(src, (map.get(src) ?? 0) + 1)
      if (tgt) map.set(tgt, (map.get(tgt) ?? 0) + 1)
    }
    return { degreeMap: map, maxDegree: Math.max(1, ...map.values()) }
  }, [links, hiddenLabels, hiddenEdgeTypes])
  const degreeMapRef = useRef(degreeMap)
  const maxDegreeRef = useRef(maxDegree)
  degreeMapRef.current = degreeMap
  maxDegreeRef.current = maxDegree

  const nodeMeshesRef = useRef<Map<string, any>>(new Map())
  
  const linkMatsRef = useRef<Map<string, any>>(new Map())
  
  const clusterSpheresRef = useRef<Map<string, any>>(new Map())
  
  const langSpheresRef = useRef<Map<string, any>>(new Map())

  const clusterMapRef = useRef<Map<string, string[]>>(new Map())
  useMemo(() => {
    const map = new Map<string, string[]>()
    for (const n of visibleNodes) {
      const c = (n.properties?.cluster as string) ?? ''
      if (c) {
        if (!map.has(c)) map.set(c, [])
        map.get(c)!.push(n.id)
      }
    }
    clusterMapRef.current = map
    return map
  
  }, [nodes, hiddenLabels, hiddenClusters])

  const langMapRef = useRef<Map<string, string[]>>(new Map())
  useMemo(() => {
    const map = new Map<string, string[]>()
    for (const n of visibleNodes) {
      const l = (n.properties?.lang as string) ?? ''
      if (l) {
        if (!map.has(l)) map.set(l, [])
        map.get(l)!.push(n.id)
      }
    }
    langMapRef.current = map
    return map
  
  }, [nodes, hiddenLabels, hiddenLangs])

  const getClusterColor = useCallback((cluster: string, idx: number) => {
    return clusterColorsRef.current[cluster] ?? CLUSTER_DEFAULT_COLORS[idx % CLUSTER_DEFAULT_COLORS.length]
  }, [])

  const getLangColor = useCallback((lang: string, idx: number) => {
    return langColorsRef.current[lang] ?? CLUSTER_DEFAULT_COLORS[(idx + 3) % CLUSTER_DEFAULT_COLORS.length]
  }, [])

  const getNodeColor = useCallback((label: string) => {
    return nodeColorsRef.current[label] ?? labelColor(label)
  }, [])

  const applySelection3D = useCallback(() => {
    const fg = graphRef.current
    if (!fg) return
    const selId = selectedIdRef.current
    const neighbourhood = selId ? getNeighbourIds(selId, visibleLinksRef.current) : null

    nodeMeshesRef.current.forEach((mesh, nodeId) => {
      if (!mesh?.material) return
      const isDimmed = neighbourhood ? !neighbourhood.has(nodeId) : false
      mesh.material.opacity = isDimmed ? 0.06 : 1
      mesh.material.needsUpdate = true
    })

    linkMatsRef.current.forEach((mat, key) => {
      const arrow = key.indexOf('→')
      const src = key.slice(0, arrow)
      const tgt = key.slice(arrow + 1)
      const highlighted = !neighbourhood || (neighbourhood.has(src) && neighbourhood.has(tgt))
      mat.color.set(highlighted ? '#60a5fa' : '#0f172a')
      mat.opacity = highlighted ? 0.9 : 0.02
      mat.needsUpdate = true
    })

    const particleFn = (l: any) => {
      if (!selId) return l.type === 'CONTAINS' ? 1 : 3
      const src = typeof l.source === 'object' ? l.source?.id : l.source
      const tgt = typeof l.target === 'object' ? l.target?.id : l.target
      return (neighbourhood?.has(src) && neighbourhood?.has(tgt)) ? (l.type === 'CONTAINS' ? 1 : 3) : 0
    }
    fg.linkDirectionalParticles?.(particleFn)
    fg.refresh?.()
  }, [])

  const cleanup3DRef = useRef<(() => void) | null>(null)

  const destroyGraph = useCallback(() => {
    if (cleanup3DRef.current) {
      cleanup3DRef.current()
      cleanup3DRef.current = null
    }
    if (graphRef.current) {
      try {
        const renderer = graphRef.current.renderer?.()
        if (renderer?.dispose) renderer.dispose()
      } catch {  }
      try { graphRef.current._destructor?.() } catch {  }
      graphRef.current = null
    }
    const el = containerRef.current
    if (el) {
      el.innerHTML = ''
    }
    nodeMeshesRef.current.clear()
    linkMatsRef.current.clear()
  }, [containerRef])

  const linkWidthFn = useCallback((l: any) => {
    const w = physicsRef.current.edgeWidth ?? 1
    const selId = selectedIdRef.current
    if (!selId) return l.type === 'CONTAINS' ? w * 0.7 : w
    const neighbourhood = neighbourhoodRef.current
    const src = linkNodeId(l.source)
    const tgt = linkNodeId(l.target)
    const connected = neighbourhood?.has(src) && neighbourhood?.has(tgt)
    return connected ? (l.type === 'CONTAINS' ? w : w * 1.5) : 0.2
  }, [])

  const build2D = useCallback(async () => {
    const el = containerRef.current
    if (!el) return
    destroyGraph()

    const mod = await import('force-graph')
    const ForceGraph: any = mod.default ?? mod
    const fg = ForceGraph()(el)

    const isDark = document.documentElement.classList.contains('dark')

    fg.width(el.clientWidth)
      .height(el.clientHeight)
      .backgroundColor('transparent')
      .nodeLabel((node: any) => {
        const n = node as GraphNode
        return n.name || n.id
      })
      .nodeCanvasObject((node: any, ctx: CanvasRenderingContext2D, globalScale: number) => {
        const n = node as GraphNode & { x: number; y: number }
        const deg = degreeMapRef.current.get(n.id) ?? 0
        const r = degreeRadius(n, deg, maxDegreeRef.current)
        const color = getNodeColor(n.label)
        const selId = selectedIdRef.current
        const isSelected = n.id === selId
        const neighbourhood = neighbourhoodRef.current
        const dimmed = neighbourhood ? !neighbourhood.has(n.id) : false

        ctx.globalAlpha = dimmed ? 0.1 : 1

        if (isSelected) {
          ctx.shadowColor = color
          ctx.shadowBlur = 20
        }
        ctx.beginPath()
        ctx.arc(n.x, n.y, r, 0, 2 * Math.PI)
        ctx.fillStyle = color
        ctx.fill()
        ctx.shadowBlur = 0

        if (isSelected) {
          ctx.strokeStyle = '#ffffff'
          ctx.lineWidth = 2.5
          ctx.stroke()
          ctx.beginPath()
          ctx.arc(n.x, n.y, r + 4, 0, 2 * Math.PI)
          ctx.strokeStyle = color + '60'
          ctx.lineWidth = 2
          ctx.stroke()
        } else if (!dimmed && neighbourhood) {
          ctx.strokeStyle = color + 'aa'
          ctx.lineWidth = 1.2
          ctx.stroke()
        }
        ctx.globalAlpha = 1

        const baseDensity = physicsRef.current.labelDensity ?? 1.2
        
        const importance = maxDegreeRef.current > 0 ? deg / maxDegreeRef.current : 0
        
        const nodeThreshold = baseDensity * (2.5 - importance * 2.35)
        const isNeighbour = neighbourhood?.has(n.id) ?? false
        const showLabel = isSelected || isNeighbour || globalScale >= nodeThreshold

        if (showLabel) {
          const label = (n.name || n.id).slice(0, 28)
          
          const fontSize = isSelected ? 15.5 / globalScale : 12 / globalScale
          ctx.font = `${isSelected ? '600 ' : ''}${fontSize}px Inter,sans-serif`
          ctx.globalAlpha = dimmed ? 0.15 : 1
          ctx.fillStyle = isDark ? '#e2e8f0' : '#1e293b'
          ctx.fillText(label, n.x + r + 2 / globalScale, n.y + 3 / globalScale)
          ctx.globalAlpha = 1
        }
      })
      .nodePointerAreaPaint((node: any, color: string, ctx: CanvasRenderingContext2D) => {
        const n = node as GraphNode & { x: number; y: number }
        const deg = degreeMapRef.current.get(n.id) ?? 0
        const r = degreeRadius(n, deg, maxDegreeRef.current)
        ctx.beginPath()
        ctx.arc(n.x, n.y, r + 5, 0, 2 * Math.PI)
        ctx.fillStyle = color
        ctx.fill()
      })
      .linkColor((l: any) => {
        const selId = selectedIdRef.current
        if (!selId) return isDark ? '#33415580' : '#cbd5e190'
        const neighbourhood = neighbourhoodRef.current
        const src = linkNodeId(l.source)
        const tgt = linkNodeId(l.target)
        const connected = neighbourhood?.has(src) && neighbourhood?.has(tgt)
        return connected ? '#60a5fa' : (isDark ? '#0f172a40' : '#f1f5f960')
      })
      .linkCurvature(0.15)
      .linkWidth(linkWidthFn)
      .linkLabel((l: any) => (l.type ?? '') as string)
      .linkDirectionalParticles((l: any) => {
        const selId = selectedIdRef.current
        if (!selId) return l.type === 'CONTAINS' ? 1 : 2
        const neighbourhood = neighbourhoodRef.current
        const src = linkNodeId(l.source)
        const tgt = linkNodeId(l.target)
        return (neighbourhood?.has(src) && neighbourhood?.has(tgt)) ? 2 : 0
      })
      .linkDirectionalParticleSpeed(0.004)
      .linkDirectionalParticleWidth((l: any) => (l.type === 'CONTAINS' ? 1.5 : 2.5))
      .linkDirectionalParticleColor((l: any) => getNodeColor((l.target as GraphNode)?.label ?? ''))
      .linkDirectionalArrowLength(0)
      .onNodeClick((node: any) => {
        onNodeClickRef.current(node)
      })
      .onBackgroundClick(() => onNodeClickRef.current(null))

    fg.d3Force('charge')?.strength(-physicsRef.current.repulsion)
    fg.d3Force('link')?.distance(physicsRef.current.linkDistance)

    const d3 = await import('d3-force')
    fg.d3Force('gravityX', d3.forceX(0).strength(physicsRef.current.gravity ?? 0.3))
    fg.d3Force('gravityY', d3.forceY(0).strength(physicsRef.current.gravity ?? 0.3))

    let cachedNodesRef: any[] | null = null
    let activeClusters: { cluster: string; nodes: any[] }[] = []
    let activeLangs: { lang: string; nodes: any[] }[] = []

    fg.onRenderFramePost((ctx: CanvasRenderingContext2D, globalScale: number) => {
      const gd = fg.graphData()
      if (!gd?.nodes?.length) return

      if (cachedNodesRef !== gd.nodes) {
        cachedNodesRef = gd.nodes
        const cMap = new Map<string, any[]>()
        const lMap = new Map<string, any[]>()
        for (const n of gd.nodes) {
          const c = n.properties?.cluster as string
          if (c && clusterMapRef.current.has(c)) {
            if (!cMap.has(c)) cMap.set(c, [])
            cMap.get(c)!.push(n)
          }
          const l = n.properties?.lang as string
          if (l && langMapRef.current.has(l)) {
            if (!lMap.has(l)) lMap.set(l, [])
            lMap.get(l)!.push(n)
          }
        }
        activeClusters = Array.from(cMap.entries()).map(([cluster, nodes]) => ({ cluster, nodes }))
        activeLangs = Array.from(lMap.entries()).map(([lang, nodes]) => ({ lang, nodes }))
      }

      let clusterIdx = 0
      for (const [cluster] of clusterMapRef.current) {
        const color = getClusterColor(cluster, clusterIdx++)
        const entry = activeClusters.find((c) => c.cluster === cluster)
        if (!entry || entry.nodes.length === 0) continue
        const bounds = clusterBounds(entry.nodes)
        if (!bounds || bounds.r < 5) continue

        ctx.save()
        
        ctx.beginPath()
        ctx.arc(bounds.cx, bounds.cy, bounds.r, 0, 2 * Math.PI)
        ctx.fillStyle = color + '12'
        ctx.fill()
        
        ctx.setLineDash([6 / globalScale, 4 / globalScale])
        ctx.strokeStyle = color + '50'
        ctx.lineWidth = 1.5 / globalScale
        ctx.stroke()
        ctx.setLineDash([])
        
        if (globalScale >= 0.4) {
          const fontSize = Math.max(8, 11 / globalScale)
          ctx.font = `600 ${fontSize}px Inter,sans-serif`
          ctx.fillStyle = color + '90'
          ctx.textAlign = 'center'
          ctx.fillText(cluster, bounds.cx, bounds.cy - bounds.r - 6 / globalScale)
          ctx.textAlign = 'start'
        }
        ctx.restore()
      }

      let langIdx = 0
      for (const [lang] of langMapRef.current) {
        const color = getLangColor(lang, langIdx++)
        const entry = activeLangs.find((l) => l.lang === lang)
        if (!entry || entry.nodes.length === 0) continue
        const bounds = clusterBounds(entry.nodes)
        if (!bounds || bounds.r < 5) continue

        ctx.save()
        
        ctx.beginPath()
        ctx.arc(bounds.cx, bounds.cy, bounds.r, 0, 2 * Math.PI)
        ctx.fillStyle = color + '0a'
        ctx.fill()
        
        ctx.setLineDash([4 / globalScale, 4 / globalScale])
        ctx.strokeStyle = color + '40'
        ctx.lineWidth = 1.0 / globalScale
        ctx.stroke()
        ctx.setLineDash([])
        
        if (globalScale >= 0.4) {
          const fontSize = Math.max(8, 11 / globalScale)
          ctx.font = `italic 500 ${fontSize}px Inter,sans-serif`
          ctx.fillStyle = color + '90'
          ctx.textAlign = 'center'
          ctx.fillText(lang, bounds.cx, bounds.cy + bounds.r + 12 / globalScale)
          ctx.textAlign = 'start'
        }
        ctx.restore()
      }
    })

    graphRef.current = fg
    const container = el as GraphCanvasRef
    container.fitGraph = () => fg.zoomToFit(400, 40)
    container.zoomBy = (factor: number) => {
      const cur = fg.zoom()
      fg.zoom(Math.max(0.1, cur * factor), 300)
    }

    fg.graphData({
      nodes: visibleNodesRef.current.map((n) => ({ ...n })),
      links: visibleLinksRef.current.map((l) => ({ ...l })),
    })

    setTimeout(() => fg.zoomToFit(400, 60), 1500)
  }, [destroyGraph, linkWidthFn, getClusterColor, getLangColor, getNodeColor])

  useEffect(() => {
    const fg = graphRef.current
    if (!fg) return
    fg.d3Force('charge')?.strength(-physics.repulsion)
    fg.d3Force('link')?.distance(physics.linkDistance)
    fg.d3Force('gravityX')?.strength(physics.gravity ?? 0.3)
    fg.d3Force('gravityY')?.strength(physics.gravity ?? 0.3)
    if (!is3D) fg.linkWidth(linkWidthFn)
    if (fg.refresh) fg.refresh()
    if (fg.d3ReheatSimulation) fg.d3ReheatSimulation()
  }, [physics, is3D, linkWidthFn])

  useEffect(() => {
    if (!graphRef.current || is3D) return
    graphRef.current.refresh?.()
    if (selectedNodeId) {
      const gd = graphRef.current.graphData?.()
      const target = gd?.nodes?.find((n: any) => n.id === selectedNodeId)
      if (target?.x != null) {
        graphRef.current.centerAt?.(target.x, target.y, 500)
      }
    }
  }, [selectedNodeId, is3D])

  const build3D = useCallback(async () => {
    const el = containerRef.current
    if (!el) return
    destroyGraph()

    let ForceGraph3D: any
    let THREE: any
    try {
      const [mod3d, modThree] = await Promise.all([
        import('3d-force-graph'),
        import('three'),
      ])
      ForceGraph3D = mod3d.default ?? mod3d
      THREE = modThree
    } catch (e) {
      console.error('3d-force-graph / three import failed:', e)
      el.innerHTML = '<div style="display:flex;align-items:center;justify-content:center;height:100%;color:#94a3b8;font-size:13px">3D not available</div>'
      return
    }

    const fg = ForceGraph3D({
      rendererConfig: {
        antialias: true,
        alpha: true,
        powerPreference: 'high-performance',
        precision: 'highp',
      },
    })(el)

    const bgColor = getBgColor()
    const applyDPR = () => fg.renderer()?.setPixelRatio(window.devicePixelRatio || 1)
    applyDPR()
    setTimeout(applyDPR, 50)

    const particleFn = (l: any) => {
      const selId = selectedIdRef.current
      if (!selId) return l.type === 'CONTAINS' ? 1 : 3
      const neighbourhood = neighbourhoodRef.current
      const src = linkNodeId(l.source)
      const tgt = linkNodeId(l.target)
      return (neighbourhood?.has(src) && neighbourhood?.has(tgt)) ? (l.type === 'CONTAINS' ? 1 : 3) : 0
    }

    fg.width(el.clientWidth)
      .height(el.clientHeight)
      .backgroundColor(bgColor)
      .nodeLabel((node: any) => {
        const n = node as GraphNode
        return n.name || n.id
      })
      
      .nodeThreeObject((node: any) => {
        const n = node as GraphNode
        const deg = degreeMapRef.current.get(n.id) ?? 0
        const r = degreeRadius(n, deg, maxDegreeRef.current) * 0.8
        const color = getNodeColor(n.label)

        const selId = selectedIdRef.current
        const neighbourhood = selId ? getNeighbourIds(selId, visibleLinksRef.current) : null
        const isDimmed = neighbourhood ? !neighbourhood.has(n.id) : false

        const geometry = new THREE.SphereGeometry(r, 32, 32)
        const material = new THREE.MeshLambertMaterial({
          color: new THREE.Color(color),
          transparent: true,
          opacity: isDimmed ? 0.06 : 1,
        })
        const mesh = new THREE.Mesh(geometry, material)
        nodeMeshesRef.current.set(n.id, mesh)
        return mesh
      })
      .nodeThreeObjectExtend(false)
      .nodeVal((node: any) => {
        const n = node as GraphNode
        const deg = degreeMapRef.current.get(n.id) ?? 0
        return degreeRadius(n, deg, maxDegreeRef.current)
      })
      .linkColor((l: any) => {
        const selId = selectedIdRef.current
        if (!selId) return '#33415580'
        const neighbourhood = neighbourhoodRef.current
        const src = linkNodeId(l.source)
        const tgt = linkNodeId(l.target)
        return (neighbourhood?.has(src) && neighbourhood?.has(tgt)) ? '#60a5fa' : '#0f172a40'
      })
      .linkCurvature(0.15)
      .linkWidth((l: any) => {
        const selId = selectedIdRef.current
        if (!selId) return l.type === 'CONTAINS' ? 0.5 : 1
        const neighbourhood = neighbourhoodRef.current
        const src = linkNodeId(l.source)
        const tgt = linkNodeId(l.target)
        return (neighbourhood?.has(src) && neighbourhood?.has(tgt)) ? 1.5 : 0.1
      })
      .linkLabel((l: any) => (l.type ?? '') as string)
      
      .linkMaterial((l: any) => {
        const src = typeof l.source === 'object' ? (l.source as any)?.id : l.source as string
        const tgt = typeof l.target === 'object' ? (l.target as any)?.id : l.target as string
        const key = `${src}→${tgt}`
        if (!linkMatsRef.current.has(key)) {
          linkMatsRef.current.set(key, new THREE.MeshBasicMaterial({
            color: new THREE.Color('#334155'),
            transparent: true,
            opacity: 0.4,
            depthWrite: false,
          }))
        }
        return linkMatsRef.current.get(key)
      })
      .linkDirectionalParticles(particleFn)
      .linkDirectionalParticleSpeed(0.004)
      .linkDirectionalParticleWidth(2.5)
      .linkDirectionalParticleColor(() => '#93c5fd')
      .linkDirectionalArrowLength(0)
      .onNodeClick((node: any) => {
        onNodeClickRef.current(node)
      })
      .onBackgroundClick(() => onNodeClickRef.current(null))

    fg.d3Force('charge')?.strength(-physicsRef.current.repulsion)
    fg.d3Force('link')?.distance(physicsRef.current.linkDistance)

    const d3_3d = await import('d3-force-3d')
    const grav = physicsRef.current.gravity ?? 0.3
    fg.d3Force('gravityX', d3_3d.forceX(0).strength(grav))
    fg.d3Force('gravityY', d3_3d.forceY(0).strength(grav))
    fg.d3Force('gravityZ', d3_3d.forceZ(0).strength(grav))

    clusterSpheresRef.current = new Map()
    const scene = fg.scene?.()
    if (scene) {
      let clusterIdx = 0
      for (const [cluster] of clusterMapRef.current) {
        const color = getClusterColor(cluster, clusterIdx++)
        const geo = new THREE.SphereGeometry(1, 32, 24)
        const mat = new THREE.MeshBasicMaterial({
          color: new THREE.Color(color),
          transparent: true,
          opacity: 0.07,
          depthWrite: false,
          side: THREE.BackSide,
        })
        const sphere = new THREE.Mesh(geo, mat)
        sphere.renderOrder = -1
        scene.add(sphere)
        clusterSpheresRef.current.set(cluster, sphere)
      }

      let langIdx = 0
      for (const [lang] of langMapRef.current) {
        const color = getLangColor(lang, langIdx++)
        const geo = new THREE.SphereGeometry(1, 32, 24)
        const mat = new THREE.MeshBasicMaterial({
          color: new THREE.Color(color),
          transparent: true,
          opacity: 0.05,
          depthWrite: false,
          side: THREE.BackSide,
        })
        const sphere = new THREE.Mesh(geo, mat)
        sphere.renderOrder = -2
        scene.add(sphere)
        langSpheresRef.current.set(lang, sphere)
      }

      const clusterAnimId = { current: 0 }
      let cachedNodesRef: any[] | null = null
      let activeClusters: { cluster: string; nodes: any[] }[] = []
      let activeLangs: { lang: string; nodes: any[] }[] = []

      const updateClusterSpheres = () => {
        clusterAnimId.current = requestAnimationFrame(updateClusterSpheres)
        const gd = fg.graphData?.()
        if (!gd?.nodes?.length) return

        if (cachedNodesRef !== gd.nodes) {
          cachedNodesRef = gd.nodes
          const cMap = new Map<string, any[]>()
          const lMap = new Map<string, any[]>()
          for (const n of gd.nodes) {
            const c = n.properties?.cluster as string
            if (c && clusterMapRef.current.has(c)) {
              if (!cMap.has(c)) cMap.set(c, [])
              cMap.get(c)!.push(n)
            }
            const l = n.properties?.lang as string
            if (l && langMapRef.current.has(l)) {
              if (!lMap.has(l)) lMap.set(l, [])
              lMap.get(l)!.push(n)
            }
          }
          activeClusters = Array.from(cMap.entries()).map(([cluster, nodes]) => ({ cluster, nodes }))
          activeLangs = Array.from(lMap.entries()).map(([lang, nodes]) => ({ lang, nodes }))
        }

        for (const [cluster, sphere] of clusterSpheresRef.current) {
          const entry = activeClusters.find((c) => c.cluster === cluster)
          if (!entry || entry.nodes.length === 0) { sphere.visible = false; continue }
          let cx = 0, cy = 0, cz = 0
          let count = 0
          for (const p of entry.nodes) {
            if (p.x != null) { cx += p.x; cy += p.y; cz += (p.z ?? 0); count++ }
          }
          if (count === 0) { sphere.visible = false; continue }
          sphere.visible = true
          cx /= count; cy /= count; cz /= count
          let maxR = 0
          for (const p of entry.nodes) {
            if (p.x != null) {
              const d = Math.sqrt((p.x - cx) ** 2 + (p.y - cy) ** 2 + ((p.z ?? 0) - cz) ** 2)
              if (d > maxR) maxR = d
            }
          }
          const padding = 25
          sphere.position.set(cx, cy, cz)
          sphere.scale.setScalar(maxR + padding)
        }

        for (const [lang, sphere] of langSpheresRef.current) {
          const entry = activeLangs.find((l) => l.lang === lang)
          if (!entry || entry.nodes.length === 0) { sphere.visible = false; continue }
          let cx = 0, cy = 0, cz = 0
          let count = 0
          for (const p of entry.nodes) {
            if (p.x != null) { cx += p.x; cy += p.y; cz += (p.z ?? 0); count++ }
          }
          if (count === 0) { sphere.visible = false; continue }
          sphere.visible = true
          cx /= count; cy /= count; cz /= count
          let maxR = 0
          for (const p of entry.nodes) {
            if (p.x != null) {
              const d = Math.sqrt((p.x - cx) ** 2 + (p.y - cy) ** 2 + ((p.z ?? 0) - cz) ** 2)
              if (d > maxR) maxR = d
            }
          }
          const padding = 20
          sphere.position.set(cx, cy, cz)
          sphere.scale.setScalar(maxR + padding)
        }
      }
      updateClusterSpheres()

      cleanup3DRef.current = () => {
        cancelAnimationFrame(clusterAnimId.current)
        for (const [, sphere] of clusterSpheresRef.current) {
          scene.remove(sphere)
          sphere.geometry?.dispose()
          sphere.material?.dispose()
        }
        for (const [, sphere] of langSpheresRef.current) {
          scene.remove(sphere)
          sphere.geometry?.dispose()
          sphere.material?.dispose()
        }
        clusterSpheresRef.current.clear()
        langSpheresRef.current.clear()
      }
    }

    graphRef.current = fg

    const container = el as GraphCanvasRef
    container.fitGraph = () => fg.zoomToFit(400)
    container.zoomBy = (factor: number) => {
      const cam = fg.camera?.()
      if (cam) fg.cameraPosition?.({ z: cam.z / factor })
    }

    fg.graphData({
      nodes: visibleNodesRef.current.map((n) => ({ ...n })),
      links: visibleLinksRef.current.map((l) => ({ ...l })),
    })

    setTimeout(() => fg.zoomToFit(400, 60), 1500)
  
  }, [destroyGraph, getClusterColor, getLangColor])

  useEffect(() => {
    if (!graphRef.current || !is3D) return
    const fg = graphRef.current
    const selId = selectedIdRef.current
    const neighbourhood = selId ? getNeighbourIds(selId, visibleLinksRef.current) : null

    requestAnimationFrame(() => {
      applySelection3D()
    })

    if (selId) {
      const gd = fg.graphData?.()
      const target = gd?.nodes?.find((n: any) => n.id === selId)
      if (target) {
        const { x = 0, y = 0, z = 0 } = target
        fg.cameraPosition?.({ x: x + 80, y: y + 80, z: z + 120 }, { x, y, z }, 600)
      }
    }
  }, [selectedNodeId, is3D, applySelection3D])

  useEffect(() => {
    if (is3D) build3D()
    else build2D()
    return destroyGraph
  }, [is3D, build2D, build3D, destroyGraph])

  useEffect(() => {
    if (!graphRef.current) return
    const fg = graphRef.current

    const currentNodes = fg.graphData().nodes
    const posMap = new Map(currentNodes.map((n: any) => [n.id, { x: n.x, y: n.y, z: n.z, vx: n.vx, vy: n.vy, vz: n.vz }]))

    const newNodes = visibleNodes.map((n) => {
      const existing = posMap.get(n.id)
      return existing ? { ...n, ...existing } : { ...n }
    })

    fg.graphData({
      nodes: newNodes,
      links: visibleLinks.map((l) => ({ ...l })),
    })
  }, [visibleNodes, visibleLinks])

  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    const ro = new ResizeObserver(() => {
      if (!graphRef.current || !el) return
      graphRef.current.width?.(el.clientWidth)
      graphRef.current.height?.(el.clientHeight)
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [containerRef])

  return (
    <div
      ref={containerRef}
      className="w-full h-full"
      style={{ background: 'transparent' }}
    />
  )
})

export function DimToggleButton({ is3D, onToggle }: { is3D: boolean; onToggle: () => void }) {
  return (
    <button
      onClick={onToggle}
      className="p-2 bg-card border border-border rounded-lg shadow-sm hover:bg-accent transition-colors flex items-center justify-center"
      title={is3D ? 'Switch to 2D view' : 'Switch to 3D view'}
    >
      <span className="text-[11px] font-semibold text-foreground leading-none w-4 text-center select-none">
        {is3D ? '3D' : '2D'}
      </span>
    </button>
  )
}
