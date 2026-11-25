import { useState, useMemo, useEffect, useRef } from 'react'
import { cn } from '@/lib/utils'
import { ChevronRight, ChevronDown, FileCode, Folder, FolderOpen } from 'lucide-react'
import type { GraphNode } from '@/api/ast'

interface NodeTreeProps {
  nodes: GraphNode[]
  projectRoot?: string
  selectedNodeId: string | null
  onNodeClick: (node: GraphNode) => void
  onFileClick: (path: string) => void
}

interface DirNode {
  name: string
  path: string
  children: Record<string, DirNode>
  files: GraphNode[]
}

function normalizeRelPath(rawPath: string, projectRoot: string): string {
  let rel = rawPath
  if (projectRoot && rel.startsWith(projectRoot)) {
    rel = rel.slice(projectRoot.length).replace(/^\//, '')
  }
  return rel
}

function buildTree(nodes: GraphNode[], projectRoot: string): DirNode {
  const root: DirNode = { name: '', path: '', children: {}, files: [] }
  const seen = new Set<string>()

  const fileNodes = nodes.filter((n) => {
    if (n.type === 'File') return true
    
    const codeEntityTypes = new Set([
      'Function', 'Class', 'Interface', 'Method', 'Variable', 'Constant',
      'Module', 'Directory', 'Type', 'Enum', 'Struct', 'Package', 'Namespace',
      'Property', 'Field', 'Parameter', 'Import', 'Export', 'Decorator',
    ])
    if (codeEntityTypes.has(n.type || '')) return false
    
    const fp = n.file || ''
    if (fp && /\.[a-zA-Z0-9]+$/.test(fp.split('/').pop() || '')) return true
    return false
  })

  const implicitDirs = new Set<string>()
  for (const node of fileNodes) {
    const rawPath = node.file || node.id || ''
    if (!rawPath) continue
    const parts = normalizeRelPath(rawPath, projectRoot).split('/').filter(Boolean)
    let acc = ''
    for (let i = 0; i < parts.length - 1; i++) {
      acc = acc ? `${acc}/${parts[i]}` : parts[i]
      implicitDirs.add(acc)
    }
  }

  for (const node of fileNodes) {
    const rawPath = node.file || node.id || ''
    if (!rawPath) continue

    const relPath = normalizeRelPath(rawPath, projectRoot)

    if (implicitDirs.has(relPath)) continue

    const parts = relPath.split('/').filter(Boolean)
    if (parts.length === 0) continue

    let cur = root
    for (let i = 0; i < parts.length - 1; i++) {
      const part = parts[i]
      if (!cur.children[part]) {
        const parentPath = cur.path ? `${cur.path}/${part}` : part
        cur.children[part] = { name: part, path: parentPath, children: {}, files: [] }
      }
      cur = cur.children[part]
    }

    const fileKey = rawPath
    if (!seen.has(fileKey)) {
      seen.add(fileKey)
      cur.files.push(node)
    }
  }

  return root
}

function getExpandedPaths(nodes: GraphNode[], selectedNodeId: string | null, projectRoot: string): Set<string> {
  if (!selectedNodeId) return new Set()
  const node = nodes.find((n) => n.id === selectedNodeId)
  if (!node) return new Set()

  const rawPath = node.file || ''
  if (!rawPath) return new Set()

  const relPath = normalizeRelPath(rawPath, projectRoot)
  const parts = relPath.split('/').filter(Boolean)
  const result = new Set<string>()
  let acc = ''
  for (let i = 0; i < parts.length - 1; i++) {
    acc = acc ? `${acc}/${parts[i]}` : parts[i]
    result.add(acc)
  }
  return result
}

export function NodeTree({ nodes, projectRoot = '', selectedNodeId, onNodeClick, onFileClick }: NodeTreeProps) {
  const tree = useMemo(() => buildTree(nodes, projectRoot), [nodes, projectRoot])

  const totalFiles = useMemo(() => {
    const uniqueFiles = new Set<string>()
    const countFiles = (dir: typeof tree) => {
      for (const f of dir.files) uniqueFiles.add(f.file || f.id)
      for (const child of Object.values(dir.children)) countFiles(child)
    }
    countFiles(tree)
    return uniqueFiles.size
  }, [tree])
  const expandedPaths = useMemo(
    () => getExpandedPaths(nodes, selectedNodeId, projectRoot),
    [nodes, selectedNodeId, projectRoot],
  )

  const selectedRef = useRef<HTMLButtonElement | null>(null)

  useEffect(() => {
    if (selectedRef.current) {
      selectedRef.current.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
    }
  }, [selectedNodeId])

  if (totalFiles === 0) {
    return (
      <p className="text-xs text-muted-foreground px-2 py-4 text-center italic">
        No file nodes in current graph
      </p>
    )
  }

  return (
    <div className="flex flex-col text-[13px] py-2">
      <RootDirEntry
        dir={tree}
        depth={0}
        selectedNodeId={selectedNodeId}
        expandedPaths={expandedPaths}
        onNodeClick={onNodeClick}
        onFileClick={onFileClick}
        selectedRef={selectedRef}
      />
    </div>
  )
}

function RootDirEntry({
  dir,
  depth,
  selectedNodeId,
  expandedPaths,
  onNodeClick,
  onFileClick,
  selectedRef,
}: {
  dir: DirNode
  depth: number
  selectedNodeId: string | null
  expandedPaths: Set<string>
  onNodeClick: (node: GraphNode) => void
  onFileClick: (path: string) => void
  selectedRef: React.RefObject<HTMLButtonElement | null>
}) {
  const forceOpen = dir.path ? expandedPaths.has(dir.path) : false
  const [open, setOpen] = useState(depth === 0 || forceOpen)
  const hasChildren = Object.keys(dir.children).length > 0 || dir.files.length > 0

  useEffect(() => {
    if (forceOpen) {
      queueMicrotask(() => setOpen(true))
    }
  }, [forceOpen])

  return (
    <div className="relative">
      {}
      {depth > 0 && open && (
        <div
          className="absolute left-3 top-6 bottom-2 w-px bg-border/40 pointer-events-none"
          style={{ left: `${8 + (depth - 1) * 16 + 7}px` }}
        />
      )}

      {dir.name && (
        <button
          className={cn(
            "flex items-center gap-2 w-full px-2 py-1.5 hover:bg-accent/40 hover:text-foreground transition-all duration-150 text-left rounded-lg text-[13px]",
            open ? "text-foreground font-semibold" : "text-muted-foreground"
          )}
          style={{ paddingLeft: `${8 + (depth - 1) * 16}px` }}
          onClick={() => setOpen((v) => !v)}
        >
          {hasChildren ? (
            open ? (
              <ChevronDown className="w-3.5 h-3.5 shrink-0 text-muted-foreground/80" />
            ) : (
              <ChevronRight className="w-3.5 h-3.5 shrink-0 text-muted-foreground/80" />
            )
          ) : (
            <span className="w-3.5" />
          )}
          {open ? (
            <FolderOpen className="w-4 h-4 text-warning shrink-0" />
          ) : (
            <Folder className="w-4 h-4 text-warning/80 shrink-0" />
          )}
          <span className="truncate">{dir.name}</span>
          {dir.files.length > 0 && (
            <span className="ml-auto font-mono text-[10px] font-bold text-muted-foreground/60 bg-accent/20 px-1.5 py-0.5 rounded border border-border/20">{dir.files.length}</span>
          )}
        </button>
      )}

      {(open || !dir.name) && (
        <div className="flex flex-col">
          {Object.values(dir.children)
            .sort((a, b) => a.name.localeCompare(b.name))
            .map((child) => (
              <RootDirEntry
                key={child.path}
                dir={child}
                depth={depth + 1}
                selectedNodeId={selectedNodeId}
                expandedPaths={expandedPaths}
                onNodeClick={onNodeClick}
                onFileClick={onFileClick}
                selectedRef={selectedRef}
              />
            ))}

          {dir.files
            .sort((a, b) => (a.name || '').localeCompare(b.name || ''))
            .map((node) => {
              const filePath = node.file || node.id || ''
              const nodeName = node.name === '<nil>' ? '' : node.name
              let fileName = filePath.split('/').pop() || nodeName || node.id
              if (fileName === '<nil>') fileName = node.id
              const isSelected = selectedNodeId === node.id
              return (
                <button
                  key={node.id}
                  ref={isSelected ? (selectedRef as React.RefObject<HTMLButtonElement>) : undefined}
                  onClick={() => {
                    onNodeClick(node)
                    if (filePath) onFileClick(filePath)
                  }}
                  style={{ paddingLeft: `${8 + depth * 16 + 14}px` }}
                  className={cn(
                    'flex items-center gap-2 w-full px-2 py-1.5 text-left transition-all duration-150 rounded-lg text-[13px] border border-transparent',
                    isSelected
                      ? 'bg-primary/10 text-primary border-primary/20 font-semibold shadow-sm'
                      : 'text-muted-foreground hover:bg-accent/40 hover:text-foreground',
                  )}
                  title={filePath}
                >
                  <FileCode className={cn(
                    "w-4 h-4 shrink-0 transition-colors",
                    isSelected ? "text-primary" : "text-muted-foreground/80"
                  )} />
                  <span className="truncate">{fileName}</span>
                </button>
              )
            })}
        </div>
      )}
    </div>
  )
}
