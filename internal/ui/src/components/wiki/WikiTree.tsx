import React, { useState, useMemo, useEffect, useRef } from 'react'
import { cn } from '@/lib/utils'
import { ChevronRight, ChevronDown, Folder, FolderOpen, FileText } from 'lucide-react'
import type { WikiPageMeta } from '@/api/wiki'

interface WikiTreeProps {
  pages: WikiPageMeta[]
  selectedPath: string | null
  onSelect: (path: string) => void
  confColor: (c: number) => string
}

interface DirNode {
  name: string
  path: string
  children: Record<string, DirNode>
  pages: WikiPageMeta[]
}

function buildTree(pages: WikiPageMeta[]): DirNode {
  const root: DirNode = { name: '', path: '', children: {}, pages: [] }

  for (const page of pages) {
    const tags = page.tags && page.tags.length > 0 ? page.tags : ['untagged']
    
    for (const tag of tags) {
      if (!root.children[tag]) {
        root.children[tag] = { name: tag, path: tag, children: {}, pages: [] }
      }
      if (!root.children[tag].pages.some(p => p.path === page.path)) {
        root.children[tag].pages.push(page)
      }
    }
  }

  return root
}

function getExpandedPaths(pages: WikiPageMeta[], selectedPath: string | null): Set<string> {
  if (!selectedPath) return new Set()
  const page = pages.find(p => p.path === selectedPath)
  if (!page) return new Set()

  const tags = page.tags && page.tags.length > 0 ? page.tags : ['untagged']
  return new Set(tags)
}

function RootDirEntry({
  dir,
  depth,
  selectedPath,
  expandedPaths,
  onSelect,
  confColor,
  selectedRef,
}: {
  dir: DirNode
  depth: number
  selectedPath: string | null
  expandedPaths: Set<string>
  onSelect: (path: string) => void
  confColor: (c: number) => string
  selectedRef: React.RefObject<HTMLButtonElement | null>
}) {
  const forceOpen = dir.path ? expandedPaths.has(dir.path) : false
  const [open, setOpen] = useState(depth === 0 || forceOpen)
  const hasChildren = Object.keys(dir.children).length > 0 || dir.pages.length > 0

  useEffect(() => {
    if (forceOpen) {
      queueMicrotask(() => setOpen(true))
    }
  }, [forceOpen])

  return (
    <div className="relative">
      {dir.name && (
        <button
          className="flex items-center gap-2 w-full px-2 py-1.5 hover:bg-accent/40 text-left rounded-lg text-xs font-semibold text-foreground/90 transition-all group my-0.5"
          style={{ paddingLeft: `${8 + depth * 12}px` }}
          onClick={() => setOpen((v) => !v)}
        >
          {hasChildren ? (
            open ? (
              <ChevronDown className="w-3.5 h-3.5 shrink-0 text-muted-foreground/60 group-hover:text-foreground transition-colors" />
            ) : (
              <ChevronRight className="w-3.5 h-3.5 shrink-0 text-muted-foreground/60 group-hover:text-foreground transition-colors" />
            )
          ) : (
            <span className="w-3.5" />
          )}
          {open ? (
            <FolderOpen className="w-4 h-4 text-primary shrink-0" />
          ) : (
            <Folder className="w-4 h-4 text-primary/75 shrink-0" />
          )}
          <span className="truncate flex-1 font-semibold">{dir.name}</span>
          {dir.pages.length > 0 && (
            <span className="text-[10px] font-bold text-muted-foreground/50 bg-accent/20 px-1.5 py-0.5 rounded border border-border/10 opacity-0 group-hover:opacity-100 transition-opacity">
              {dir.pages.length}
            </span>
          )}
        </button>
      )}

      {(open || !dir.name) && (
        <div className={cn(dir.name && "relative ml-3.5 border-l border-border/25 pl-1.5")}>
          {Object.values(dir.children)
            .sort((a, b) => a.name.localeCompare(b.name))
            .map((child) => (
              <RootDirEntry
                key={child.path}
                dir={child}
                depth={dir.name ? depth + 1 : depth}
                selectedPath={selectedPath}
                expandedPaths={expandedPaths}
                onSelect={onSelect}
                confColor={confColor}
                selectedRef={selectedRef}
              />
            ))}

          {dir.pages
            .sort((a, b) => a.title.localeCompare(b.title))
            .map((page) => {
              const isSelected = selectedPath === page.path
              return (
                <button
                  key={page.path}
                  ref={isSelected ? selectedRef as React.RefObject<HTMLButtonElement> : undefined}
                  onClick={() => onSelect(page.path)}
                  style={{ paddingLeft: `${8 + (dir.name ? depth + 1 : depth) * 12}px` }}
                  className={cn(
                    'flex items-center gap-2 w-full px-2.5 py-1.5 text-left hover:bg-accent/40 transition-all duration-150 rounded-lg text-xs group my-0.5 border border-transparent',
                    isSelected
                      ? 'bg-primary/10 text-primary border-primary/20 font-bold shadow-sm'
                      : 'text-muted-foreground hover:text-foreground',
                  )}
                  title={page.title}
                >
                  <FileText className={cn(
                    "w-3.5 h-3.5 shrink-0 transition-transform group-hover:scale-105",
                    isSelected ? 'text-primary' : 'text-muted-foreground/60'
                  )} />
                  <span className="truncate flex-1 font-medium">{page.title}</span>
                  {page.confidence > 0 && (
                    <span
                      title={`Confidence: ${Math.round(page.confidence * 100)}%`}
                      className={cn('w-2 h-2 rounded-full shrink-0 border border-black/10 shadow-sm', confColor(page.confidence))}
                    />
                  )}
                </button>
              )
            })}
        </div>
      )}
    </div>
  )
}

export function WikiTree({ pages, selectedPath, onSelect, confColor }: WikiTreeProps) {
  const tree = useMemo(() => buildTree(pages), [pages])
  const expandedPaths = useMemo(
    () => getExpandedPaths(pages, selectedPath),
    [pages, selectedPath],
  )

  const selectedRef = useRef<HTMLButtonElement | null>(null)

  useEffect(() => {
    if (selectedRef.current) {
      selectedRef.current.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
    }
  }, [selectedPath])

  if (pages.length === 0) return null

  return (
    <div className="flex flex-col text-xs mt-1 border-t border-border/20 pt-2 space-y-0.5">
      <RootDirEntry
        dir={tree}
        depth={0}
        selectedPath={selectedPath}
        expandedPaths={expandedPaths}
        onSelect={onSelect}
        confColor={confColor}
        selectedRef={selectedRef}
      />
    </div>
  )
}
