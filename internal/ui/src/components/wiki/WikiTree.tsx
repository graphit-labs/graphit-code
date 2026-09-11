import React, { useState, useMemo, useEffect, useRef } from 'react'
import { cn } from '@/lib/utils'
import { FileText, ChevronDown, ChevronRight } from 'lucide-react'
import type { WikiPageMeta } from '@/api/wiki'

interface WikiTreeProps {
  pages: WikiPageMeta[]
  selectedPath: string | null
  onSelect: (path: string) => void
  confColor: (c: number) => string
}

interface FlatNode {
  page: WikiPageMeta
  displayTitle: string
  parentPath: string | null
  children: FlatNode[]
}

function buildHierarchy(pages: WikiPageMeta[]): FlatNode[] {
  const pageMap = new Map<string, WikiPageMeta>()
  pages.forEach(p => pageMap.set(p.title, p))

  const nodes: FlatNode[] = pages.map(p => {
    let parentPath: string | null = null
    let displayTitle = p.title

    const idx = p.title.indexOf(' - ')
    if (idx > 0) {
      const parentTitle = p.title.substring(0, idx)
      const parentPage = pageMap.get(parentTitle)
      if (parentPage) {
        parentPath = parentPage.path
        displayTitle = p.title.substring(idx + 3)
      }
    }

    return {
      page: p,
      displayTitle,
      parentPath,
      children: []
    }
  })

  const nodeMap = new Map<string, FlatNode>()
  nodes.forEach(n => nodeMap.set(n.page.path, n))

  const rootNodes: FlatNode[] = []
  nodes.forEach(n => {
    if (n.parentPath) {
      const parentNode = nodeMap.get(n.parentPath)
      if (parentNode) {
        parentNode.children.push(n)
      } else {
        rootNodes.push(n)
      }
    } else {
      rootNodes.push(n)
    }
  })

  rootNodes.sort((a, b) => a.page.title.localeCompare(b.page.title))
  nodes.forEach(n => {
    n.children.sort((a, b) => a.page.title.localeCompare(b.page.title))
  })

  return rootNodes
}

function NodeEntry({
  node,
  selectedPath,
  onSelect,
  confColor,
  selectedRef,
}: {
  node: FlatNode
  selectedPath: string | null
  onSelect: (path: string) => void
  confColor: (c: number) => string
  selectedRef: React.RefObject<HTMLButtonElement | null>
}) {
  const hasChildren = node.children.length > 0
  const isSelected = selectedPath === node.page.path
  const [open, setOpen] = useState(false)

  const hasSelectedChild = useMemo(() => {
    if (!selectedPath) return false
    const checkSelected = (n: FlatNode): boolean => {
      if (n.page.path === selectedPath) return true
      return n.children.some(checkSelected)
    }
    return node.children.some(checkSelected)
  }, [node.children, selectedPath])

  useEffect(() => {
    if (hasSelectedChild) {
      const id = setTimeout(() => setOpen(true), 0)
      return () => clearTimeout(id)
    }
  }, [hasSelectedChild])

  return (
    <div className="flex flex-col">
      <div className="group relative flex items-center w-full my-0.5">
        {hasChildren && (
          <button
            onClick={(e) => {
              e.stopPropagation()
              setOpen(!open)
            }}
            className="absolute left-1 p-0.5 rounded hover:bg-accent/40 text-muted-foreground/60 hover:text-foreground z-10"
          >
            {open ? (
              <ChevronDown className="w-3 h-3" />
            ) : (
              <ChevronRight className="w-3 h-3" />
            )}
          </button>
        )}

        <button
          ref={isSelected ? selectedRef as React.RefObject<HTMLButtonElement> : undefined}
          onClick={() => onSelect(node.page.path)}
          className={cn(
            'flex items-center gap-2 w-full py-1.5 text-left hover:bg-accent/45 transition-all duration-150 rounded-lg text-xs border border-transparent',
            hasChildren ? 'pl-6' : 'pl-3',
            isSelected
              ? 'bg-primary/10 text-primary border-primary/20 font-bold shadow-sm'
              : 'text-muted-foreground hover:text-foreground',
          )}
          title={node.page.title}
        >
          <FileText className={cn(
            "w-3.5 h-3.5 shrink-0 transition-transform group-hover:scale-105",
            isSelected ? 'text-primary' : 'text-muted-foreground/50'
          )} />
          <span className="truncate flex-1 font-medium">{node.displayTitle}</span>
          {node.page.confidence > 0 && (
            <span
              title={`Confidence: ${Math.round(node.page.confidence * 100)}%`}
              className={cn('w-2 h-2 rounded-full shrink-0 border border-black/10 shadow-sm mr-1.5', confColor(node.page.confidence))}
            />
          )}
        </button>
      </div>

      {hasChildren && open && (
        <div className="relative ml-4 border-l border-border/25 pl-2 space-y-0.5 animate-in slide-in-from-top-1 duration-150">
          {node.children.map((child) => (
            <NodeEntry
              key={child.page.path}
              node={child}
              selectedPath={selectedPath}
              onSelect={onSelect}
              confColor={confColor}
              selectedRef={selectedRef}
            />
          ))}
        </div>
      )}
    </div>
  )
}

export function WikiTree({ pages, selectedPath, onSelect, confColor }: WikiTreeProps) {
  const rootNodes = useMemo(() => buildHierarchy(pages), [pages])
  const selectedRef = useRef<HTMLButtonElement | null>(null)

  useEffect(() => {
    if (selectedRef.current) {
      selectedRef.current.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
    }
  }, [selectedPath])

  if (pages.length === 0) return null

  return (
    <div className="flex flex-col text-xs mt-1 border-t border-border/20 pt-2 space-y-0.5 max-h-[calc(100vh-280px)] overflow-y-auto pr-1 select-none scrollbar-thin">
      {rootNodes.map(node => (
        <NodeEntry
          key={node.page.path}
          node={node}
          selectedPath={selectedPath}
          onSelect={onSelect}
          confColor={confColor}
          selectedRef={selectedRef}
        />
      ))}
    </div>
  )
}
