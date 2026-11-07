import '@testing-library/jest-dom/vitest'
import { render, screen } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { KnowledgePageCatalogue } from './KnowledgePageCatalogue'

afterEach(() => vi.restoreAllMocks())

it.each([
  {name:'desktop', viewportTop:64, headerBottom:64, articleTop:542.58, scrollTop:200, height:780, margin:0},
  {name:'fresh narrow viewport', viewportTop:100.5, headerBottom:100.5, articleTop:400, scrollTop:500, height:744, margin:0},
  {name:'narrow viewport already behind sticky header', viewportTop:0, headerBottom:100.5, articleTop:-0.1, scrollTop:38395, height:744, margin:16},
])('reveals only inside the WorkPage and respects the visible header boundary: $name', geometry => {
  HTMLElement.prototype.scrollIntoView = vi.fn()
  HTMLElement.prototype.scrollTo = vi.fn()
  window.scrollTo = vi.fn()
  vi.spyOn(HTMLElement.prototype,'getBoundingClientRect').mockImplementation(function(this:HTMLElement) {
    if (this.matches('.workspace-header')) return new DOMRect(0,0,390,geometry.headerBottom)
    if (this.matches('.work-page')) return new DOMRect(0,geometry.viewportTop,390,geometry.height)
    if (this.matches('article')) return new DOMRect(0,geometry.articleTop,390,50)
    return new DOMRect()
  })
  const view = (slug:string|null) => <div className="workspace-shell"><style>{`.knowledge-inline-reader { scroll-margin-top: ${geometry.margin}px; }`}</style><header className="workspace-header" /><div className="work-page"><KnowledgePageCatalogue heads={[{slug:'guide',title:'Guide',updated_at:null}]} slug={slug} onSelect={vi.fn()} label="Pages" readerLabel="Selected document">Loading document…</KnowledgePageCatalogue></div></div>
  const rendered = render(view(null))
  const viewport = rendered.container.querySelector<HTMLElement>('.work-page')!
  Object.defineProperty(viewport,'clientHeight',{value:geometry.height,configurable:true})
  viewport.scrollTop = geometry.scrollTop
  rendered.rerender(view('guide'))
  const article = screen.getByRole('article',{name:'Selected document'})
  expect(article).toHaveFocus()
  expect(article.style.minHeight).toBe(`${geometry.height}px`)
  const visibleTop = Math.max(geometry.viewportTop,geometry.headerBottom) + geometry.margin
  expect(HTMLElement.prototype.scrollTo).toHaveBeenCalledTimes(1)
  expect(HTMLElement.prototype.scrollTo).toHaveBeenCalledWith({top:geometry.scrollTop+geometry.articleTop-visibleTop,behavior:'instant'})
  expect(vi.mocked(HTMLElement.prototype.scrollTo).mock.contexts[0]).toBe(viewport)
  expect(HTMLElement.prototype.scrollIntoView).not.toHaveBeenCalled()
  expect(window.scrollTo).not.toHaveBeenCalled()
})
