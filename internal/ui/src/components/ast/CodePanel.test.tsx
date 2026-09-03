import { render, screen } from '@testing-library/react'
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { CodePanel } from './CodePanel'

const FILE = ['package main', '', 'func Alpha() {}', '', 'func Beta() {}'].join('\n')

let scrolled: Element[] = []

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
