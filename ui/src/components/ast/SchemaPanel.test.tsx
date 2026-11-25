import { render, screen, within } from '@testing-library/react'
import { describe, it, expect, vi } from 'vitest'
import { SchemaPanel } from './SchemaPanel'
import { NO_LANG } from '@/lib/utils'
import type { SchemaLangGroup, SchemaNodeStat } from '@/api/ast'

const LANGS: SchemaLangGroup[] = [
  { lang: 'go', count: 5, labels: [{ label: 'Function', count: 3 }, { label: 'Comment', count: 2 }] },
  { lang: 'css', count: 2, labels: [{ label: 'CssClass', count: 2 }] },
  { lang: '', count: 5, labels: [{ label: 'Function', count: 4 }, { label: 'Table', count: 1 }] },
]

const FLAT: SchemaNodeStat[] = [
  { label: 'Function', count: 7 },
  { label: 'Comment', count: 2 },
  { label: 'CssClass', count: 2 },
]

function renderPanel(over: Partial<React.ComponentProps<typeof SchemaPanel>> = {}) {
  const props: React.ComponentProps<typeof SchemaPanel> = {
    nodes: FLAT,
    edges: [],
    langs: LANGS,
    graphNodes: [],
    hiddenLabels: new Set<string>(),
    hiddenEdgeTypes: new Set<string>(),
    hiddenClusters: new Set<string>(),
    hiddenLangs: new Set<string>(),
    collapsedLangs: new Set<string>(),
    nodeColors: {},
    clusterColors: {},
    langColors: {},
    onToggleLabel: vi.fn(),
    onToggleEdge: vi.fn(),
    onToggleCluster: vi.fn(),
    onToggleLang: vi.fn(),
    onToggleLangCollapse: vi.fn(),
    onColorChange: vi.fn(),
    onClusterColorChange: vi.fn(),
    onLangColorChange: vi.fn(),
    ...over,
  }
  return { ...render(<SchemaPanel {...props} />), props }
}

function group(lang: string): HTMLElement {
  const name = screen.getByText(lang)
  const box = name.closest('div.rounded-lg.border')
  if (!box) throw new Error(`no group box around ${lang}`)
  return box as HTMLElement
}

describe('SchemaPanel', () => {
  it('nests each language’s labels under that language', () => {
    renderPanel()

    expect(within(group('go')).getByText('Function')).toBeTruthy()
    expect(within(group('go')).getByText('Comment')).toBeTruthy()
    expect(within(group('go')).queryByText('CssClass')).toBeNull()

    expect(within(group('css')).getByText('CssClass')).toBeTruthy()
    expect(within(group('css')).queryByText('Function')).toBeNull()
  })

  it('shows a label under every language that produced it, with that language’s count', () => {
    renderPanel()

    expect(within(group('go')).getByText('3')).toBeTruthy()
    expect(within(group(NO_LANG)).getByText('4')).toBeTruthy()
    expect(screen.queryByText('7')).toBeNull()
  })

  it('names the language-less group and keys its hide toggle off that name', () => {
    const { props } = renderPanel()

    const box = group(NO_LANG)
    const hide = within(box).getByTitle('Hide language')
    hide.click()

    expect(props.onToggleLang).toHaveBeenCalledWith(NO_LANG)
  })

  it('gives the language-less group no colour picker — the canvas draws no hull for it', () => {
    renderPanel()

    expect(within(group('go')).getByTitle('Change language color')).toBeTruthy()
    expect(within(group(NO_LANG)).queryByTitle('Change language color')).toBeNull()
  })

  it('hides a collapsed language’s labels but keeps the language itself', () => {
    renderPanel({ collapsedLangs: new Set(['go']) })

    expect(screen.getByText('go')).toBeTruthy()
    expect(within(group('go')).queryByText('Function')).toBeNull()
    expect(within(group('go')).queryByText('Comment')).toBeNull()

    expect(within(group('css')).getByText('CssClass')).toBeTruthy()
  })

  it('collapsing is a request to the page, not local state', () => {
    const { props } = renderPanel()

    within(group('css')).getByTitle('Collapse').click()

    expect(props.onToggleLangCollapse).toHaveBeenCalledWith('css')
  })

  it('falls back to the flat list when the server sent no grouping', () => {
    renderPanel({ langs: [] })

    expect(screen.getByText('Node Labels')).toBeTruthy()
    expect(screen.getByText('Function')).toBeTruthy()
    expect(screen.getByText('7')).toBeTruthy()
    expect(screen.queryByText(NO_LANG)).toBeNull()
  })

  it('still toggles a label from inside a group', () => {
    const { props } = renderPanel()

    const row = within(group('css')).getByText('CssClass').closest('div')!
    within(row as HTMLElement).getByTitle('Hide').click()

    expect(props.onToggleLabel).toHaveBeenCalledWith('CssClass')
  })
})
