import { render, screen, fireEvent } from '@testing-library/react'
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { CodePanel } from './CodePanel'

const FILE = ['package main', '', 'func Alpha() {}', '', 'func Beta() {}'].join('\n')

let scrolled: Element[] = []
afterEach(() => vi.unstubAllGlobals())

beforeEach(() => {
  scrolled = []
  Element.prototype.scrollIntoView = vi.fn(function (this: Element) {
    scrolled.push(this)
  })
})

function lineEl(oneBased: number): HTMLElement {
  const lines = document.querySelectorAll<HTMLElement>('.code-line')
  return lines[oneBased - 1]
}

describe('CodePanel', () => {
  it('renders global native line coordinates and removes only the trailing split artifact', () => {
    const source = 'first\nsecond\n'
    render(<CodePanel content={source} filename="main.rs" highlightLine={43} sourceSelection={{ source, total_lines: 200, start_line: 42, end_line: 43 }} />)
    const lines = document.querySelectorAll<HTMLElement>('.code-line')
    expect([...lines].map(line => line.dataset.lineNumber)).toEqual(['42', '43'])
    expect(lines[1].className).toContain('target-line')
    expect(scrolled[0]).toBe(lines[1])
  })
  it('preserves large global coordinates without rounding', () => {
    // Transport/display fixture; no claim that this file exists in the SDK corpus.
    const start = 9007199254740993n, source = 'first\nsecond'
    render(<CodePanel content={source} filename="main.rs" highlightLine={start + 1n} sourceSelection={{ source, total_lines: start + 1n, start_line: start, end_line: start + 1n }} />)
    const lines = document.querySelectorAll<HTMLElement>('.code-line')
    expect([...lines].map(line => line.dataset.lineNumber)).toEqual(['9007199254740993', '9007199254740994'])
    expect(lines[1].className).toContain('target-line')
  })
  it('renders structured SDK matches once and copies the literal formatted source', async () => {
    const source = '4 > matched\n---\n10 context', writeText = vi.fn().mockResolvedValue(undefined)
    vi.stubGlobal('navigator', { clipboard: { writeText } })
    render(<CodePanel content={source} filename="main.rs" sourceSelection={{ source, total_lines: 30, matches: [{ line_number: 4, line: 'matched', is_match: true }, { line_number: 10, line: 'context', is_match: false }] }} />)
    const lines = document.querySelectorAll<HTMLElement>('.code-line')
    expect([...lines].map(line => line.dataset.lineNumber)).toEqual(['4', '10'])
    expect(lines[0].dataset.sdkMatch).toBe('true')
    expect(lines[0].textContent).not.toContain('4 >')
    fireEvent.click(screen.getByTitle('Copy Code'))
    expect(writeText).toHaveBeenCalledWith(source)
  })
  it('does not add invented gutters to source already formatted by the SDK', () => {
    const source = '42\tfirst'
    render(<CodePanel content={source} filename="main.rs" formattedSource sourceSelection={{ source, total_lines: 200, start_line: 42, end_line: 42 }} />)
    const line = document.querySelector<HTMLElement>('.code-line')!
    expect(line.dataset.lineNumber).toBeUndefined()
    expect(line.querySelector('.code-gutter')?.textContent).toBe('')
    expect(line.textContent).toContain('42')
  })
  it('keeps empty native source and omitted coordinates distinct from line one', () => {
    const view = render(<CodePanel content="" filename="main.rs" sourceSelection={{ source: '', total_lines: 0 }} />)
    expect(document.querySelectorAll('.code-line')).toHaveLength(0)
    view.rerender(<CodePanel content="literal" filename="main.rs" sourceSelection={{ source: 'literal', total_lines: 1 }} />)
    expect(document.querySelector<HTMLElement>('.code-line')?.dataset.lineNumber).toBeUndefined()
  })
  it('marks the requested line and scrolls it into view', () => {
    render(<CodePanel content={FILE} filename="main.go" highlightLine={3} />)

    expect(lineEl(3).className).toContain('target-line')
    expect(scrolled).toHaveLength(1)
    expect(scrolled[0]).toBe(lineEl(3))
  })

  it('marks only that line', () => {
    render(<CodePanel content={FILE} filename="main.go" highlightLine={3} />)

    const marked = document.querySelectorAll('.target-line')
    expect(marked).toHaveLength(1)
    expect(marked[0].textContent).toContain('func Alpha')
  })

  it('opens at the top when there is no line — a file has no declaration', () => {
    render(<CodePanel content={FILE} filename="main.go" highlightLine={null} />)

    expect(document.querySelectorAll('.target-line')).toHaveLength(0)
    expect(scrolled).toHaveLength(0)
  })

  it('scrolls when the content arrives after the line', () => {
    const { rerender } = render(
      <CodePanel content="" filename="main.go" highlightLine={5} />
    )
    expect(scrolled).toHaveLength(0)

    rerender(<CodePanel content={FILE} filename="main.go" highlightLine={5} />)

    expect(scrolled).toHaveLength(1)
    expect(lineEl(5).className).toContain('target-line')
    expect(lineEl(5).textContent).toContain('func Beta')
  })

  it('moves when only the line changes', () => {
    const { rerender } = render(
      <CodePanel content={FILE} filename="main.go" highlightLine={3} />
    )
    rerender(<CodePanel content={FILE} filename="main.go" highlightLine={5} />)

    expect(scrolled).toHaveLength(2)
    expect(scrolled[1]).toBe(lineEl(5))
    expect(document.querySelectorAll('.target-line')).toHaveLength(1)
  })

  it('survives a line beyond the end of the file', () => {
    render(<CodePanel content={FILE} filename="main.go" highlightLine={999} />)

    expect(document.querySelectorAll('.target-line')).toHaveLength(0)
    expect(scrolled).toHaveLength(0)
    expect(screen.getByTitle('main.go')).toBeDefined()
  })
})
