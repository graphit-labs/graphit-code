import { render, screen, within } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { expect, it, vi } from 'vitest'
import type { NativeKnowledgePage } from '@/api/nativeKnowledge'
import { NativeKnowledgeDocument } from './NativeKnowledgeDocument'

vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }))

function publication(body: string, title = 'Guia 日本語'): Extract<NativeKnowledgePage, {state:'published'}> {
  return {state:'published',page:{state:'found',document:{
    slug:'guide',title,body,resolved_wiki_targets:[],
  }}}
}

it('shows one title and keeps its focus while Raw and Copy retain the literal document', async () => {
  const user = userEvent.setup()
  const body = '---\r\ntitle: Guia 日本語\r\n---\r\n\r\n# Guia 日本語\r\n\r\n## Section\r\n\r\nNative **body**.\r\n'
  const native = publication(body)
  render(<NativeKnowledgeDocument publication={native} openPage={vi.fn()} browseIndex={vi.fn()} />)
  const headings = screen.getAllByRole('heading', {level:1,name:'Guia 日本語'})
  expect(headings).toHaveLength(1)
  expect(headings[0]).toHaveFocus()
  expect(headings[0]).toHaveAttribute('tabindex','-1')
  expect(screen.getByRole('heading', {name:'Section'})).toBeTruthy()
  const bodySection = screen.getByRole('region', {name:'Document body'})
  expect(bodySection.textContent).not.toContain('title: Guia 日本語')
  await user.click(screen.getByRole('button', {name:'Raw Markdown'}))
  expect(bodySection.querySelector('pre')!.textContent).toBe(body)
  const copy = vi.spyOn(navigator.clipboard,'writeText')
  await user.click(screen.getByRole('button', {name:'Copy Markdown'}))
  expect(copy).toHaveBeenCalledWith(body)
  expect(screen.queryByText('Complete publication document')).toBeNull()
  await user.click(screen.getByRole('button', {name:'Rendered document'}))
  expect(within(bodySection).queryByRole('heading', {level:1})).toBeNull()
})

it.each([
  ['Body without a title.', 'Guia 日本語', null],
  ['# A different authored title\n\nBody.', 'Guia 日本語', 'A different authored title'],
  ['Body without a title.', '', null],
])('retains the metadata title and focus when the body has no duplicate H1: %s', (body,title,authoredTitle) => {
  render(<NativeKnowledgeDocument publication={publication(body,title)} openPage={vi.fn()} browseIndex={vi.fn()} />)
  const metadata = screen.getByRole('heading', {level:1,name:title || 'guide'})
  expect(metadata).toHaveFocus()
  if (authoredTitle) expect(screen.getByRole('heading', {level:1,name:authoredTitle})).toBeTruthy()
})

it('links the outline to rendered headings, including repeated titles and non-Latin text', () => {
  render(<NativeKnowledgeDocument publication={publication('## Visão geral\n\nText.\n\n## Visão geral\n\n```text\n## Code, not a heading\n```')} openPage={vi.fn()} browseIndex={vi.fn()} />)
  const outline = screen.getByRole('navigation', { name: 'On this page' })
  const headings = screen.getAllByRole('heading', { name: 'Visão geral' })
  const links = within(outline).getAllByRole('link', { name: 'Visão geral' })
  expect(headings.map(heading => heading.id)).toEqual(['visão-geral', 'visão-geral-2'])
  expect(links.map(link => decodeURIComponent(link.getAttribute('href')!))).toEqual(['#visão-geral', '#visão-geral-2'])
  expect(within(outline).getAllByRole('link')).toHaveLength(2)
  expect(screen.queryByRole('navigation', { name: 'Document sequence' })).toBeNull()
})

it('links authored Markdown paths to the exact page in the current publication and preserves its fragment', () => {
  const native = publication('[Install](../setup/first%20steps.md#install) · [Missing](../missing.md) · [Escape](../../outside.md) · [External](https://example.test/info.md)')
  if (native.page.state !== 'found') throw new Error('Expected a document fixture')
  native.page.document.source_path = 'docs/guide.md'
  native.page.document.resolved_wiki_targets = ['exact-target', 'unrelated']
  const heads = [
    { slug: 'exact-target', title: 'First steps', updated_at: null, source_path: 'setup/first steps.md' },
    { slug: 'unrelated', title: 'Same filename', updated_at: null, source_path: 'other/first steps.md' },
  ]
  render(<MemoryRouter><NativeKnowledgeDocument publication={native} openPage={vi.fn()} browseIndex={vi.fn()}
    heads={heads} pageHref={slug => '/knowledge/explorer?library=imported&page=' + slug} /></MemoryRouter>)
  expect(screen.getByRole('link', { name: 'Install' })).toHaveAttribute('href', '/knowledge/explorer?library=imported&page=exact-target#install')
  expect(screen.getByRole('link', { name: 'External' })).toHaveAttribute('href', 'https://example.test/info.md')
  expect(screen.queryByRole('link', { name: /Missing|Escape/ })).toBeNull()
})

it('focuses the referenced heading when a document opens with an encoded fragment', () => {
  render(<NativeKnowledgeDocument publication={publication('## Instalação')} openPage={vi.fn()} browseIndex={vi.fn()} fragment="#instala%C3%A7%C3%A3o" />)
  expect(screen.getByRole('heading', { name: 'Instalação' })).toHaveFocus()
})

it('opens the authored next page from the header and keeps it out of related pages', async () => {
  const native = publication('First part of a guide.')
  if (native.page.state !== 'found') throw new Error('Expected a document fixture')
  native.page.document.next = { target: 'next.md', slug: 'next', title: 'Next step' }
  native.page.document.resolved_wiki_targets = ['next', 'related', 'related', 'unknown']
  const openPage = vi.fn()
  render(<NativeKnowledgeDocument publication={native} openPage={openPage} browseIndex={vi.fn()}
    heads={[{ slug: 'next', title: 'Next step', updated_at: null }, { slug: 'related', title: 'Related topic', updated_at: null }]} />)
  expect(screen.getByText('Reading path')).toBeVisible()
  const user = userEvent.setup()
  await user.click(screen.getByRole('button', { name: 'Up next Next step' }))
  expect(openPage).toHaveBeenLastCalledWith('next')
  const related = screen.getByRole('navigation', { name: 'Related pages' })
  const links = within(related).getAllByRole('button')
  expect(links).toHaveLength(1)
  await user.click(links[0])
  expect(openPage).toHaveBeenLastCalledWith('related')
})

it('returns independent pages and the end of an authored path to the page index', async () => {
  const native = publication('Reading material.')
  if (native.page.state !== 'found') throw new Error('Expected a document fixture')
  const browseIndex = vi.fn()
  const user = userEvent.setup()
  const { rerender } = render(<NativeKnowledgeDocument publication={native} openPage={vi.fn()} browseIndex={browseIndex} />)
  expect(screen.getByText('Independent page')).toBeVisible()
  expect(screen.queryByRole('navigation', { name: 'Document sequence' })).toBeNull()
  expect(screen.queryByRole('navigation', { name: 'Related pages' })).toBeNull()
  await user.click(screen.getByRole('button', { name: 'Browse page index' }))
  expect(browseIndex).toHaveBeenCalledTimes(1)

  native.page.document.previous = { target: 'first.md', slug: 'first', title: 'First step' }
  rerender(<NativeKnowledgeDocument publication={native} openPage={vi.fn()} browseIndex={browseIndex} />)
  expect(screen.getByText('Reading path')).toBeVisible()
  expect(screen.getByRole('button', { name: 'Previous page: First step' })).toBeVisible()
  expect(screen.queryByRole('button', { name: /Next page:/ })).toBeNull()
  await user.click(screen.getByRole('button', { name: 'Browse page index' }))
  expect(browseIndex).toHaveBeenCalledTimes(2)
})
