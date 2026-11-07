import { fireEvent, render, screen, within } from '@testing-library/react'
import { expect, it, vi } from 'vitest'
import { MarkdownContent } from './WikiMarkdown'

vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }))

it.each(['\n', '\r\n'])('recognizes initial YAML only for opted-in documents (%j)', newline => {
  const content = ['---','title: MetadataOnly','tags: [guide]','---','','# Guide','','| Check | Result |','| --- | --- |','| Scope | Preserved |'].join(newline)
  const view = render(<MarkdownContent content={content} frontmatter />)
  expect(screen.getByRole('heading', {name:'Guide'})).toBeTruthy()
  expect(screen.getByRole('table').textContent).toContain('Preserved')
  expect(view.container.textContent).not.toContain('MetadataOnly')
  view.rerender(<MarkdownContent content={content} />)
  expect(view.container.textContent).toContain('MetadataOnly')
})

it('keeps ordinary rules, later Markdown and fenced YAML visible in document mode', () => {
  const content = '# Guide\n\n---\n\nBody\n\n---\nlater: Literal section\n---\n\n```yaml\n---\nexample: literal\n---\n```'
  const view = render(<MarkdownContent content={content} frontmatter />)
  expect(screen.getAllByRole('separator').length).toBeGreaterThan(0)
  expect(view.container.textContent).toContain('later: Literal section')
  expect(view.container.querySelector('pre')?.textContent).toContain('example: literal')
})

it('opens an image preview by keyboard in the shared renderer and closes it by Escape', async () => {
  render(<MarkdownContent content="![Published diagram](/published/diagram.svg)" />)
  const image = screen.getByRole('button', { name: 'Published diagram' })
  fireEvent.keyDown(image, { key: 'Enter' })
  const preview = await screen.findByRole('dialog', { name: 'Image preview' })
  expect(within(preview).getByRole('img').getAttribute('src')).toBe('/published/diagram.svg')
  fireEvent.keyDown(document, { key: 'Escape' })
  expect(screen.queryByRole('dialog')).toBeNull()
})

it('opens a rendered diagram without an explorer event listener', async () => {
  render(<MarkdownContent content={'```plantuml\nAlice -> Bob\n```'} />)
  fireEvent.click(screen.getByRole('button', { name: 'PlantUML diagram' }))
  const preview = await screen.findByRole('dialog', { name: 'Image preview' })
  expect(within(preview).getByRole('img').getAttribute('src')).toContain('https://www.plantuml.com/plantuml/svg/~h')
  fireEvent.click(within(preview).getByRole('button', { name: 'Close image preview' }))
  expect(screen.queryByRole('dialog')).toBeNull()
})

it.each(['\n', '\r\n'])('omits only a leading title equal to the displayed title using parsed Markdown (%j)', newline => {
  const content = ['---', 'title: ignored metadata', '---', '', '# **Árvore** &amp; `Rust`', '', 'Corpo 日本語.', '', '[Section](#section)', '', '## Section', '', '# Another title'].join(newline)
  const view = render(<MarkdownContent content={content} frontmatter displayedTitle="Árvore & Rust" />)
  expect(screen.queryByRole('heading', {name:'Árvore & Rust'})).toBeNull()
  expect(screen.getByRole('heading', {name:'Another title'})).toBeTruthy()
  expect(screen.getByRole('heading', {name:'Section'})).toBeTruthy()
  expect(screen.getByRole('link', {name:'Section'}).getAttribute('href')).toBe('#section')
  expect(view.container.textContent).toContain('Corpo 日本語.')
  view.rerender(<MarkdownContent content={content} frontmatter />)
  expect(screen.getByRole('heading', {name:'Árvore & Rust'})).toBeTruthy()
})

it.each([
  ['# Different title', 'Guide', 'Different title'],
  ['# GUIDE', 'Guide', 'GUIDE'],
  ['# Cafe\u0301', 'Café', 'Cafe\u0301'],
  ['Introduction.\n\n# Guide', 'Guide', 'Guide'],
  ['> # Guide', 'Guide', 'Guide'],
  ['## Section\n\n# Guide', 'Guide', 'Guide'],
])('preserves an authored heading outside the exact leading-title case: %s', (content, displayedTitle, heading) => {
  render(<MarkdownContent content={content} frontmatter displayedTitle={displayedTitle} />)
  expect(screen.getByRole('heading', {name:heading})).toBeTruthy()
})

it('recognizes a leading setext H1 and preserves later identical headings', () => {
  render(<MarkdownContent content={'Guide\n=====\n\n# Guide\n\n## Kept'} frontmatter displayedTitle="Guide" />)
  expect(screen.getAllByRole('heading', {name:'Guide'})).toHaveLength(1)
  expect(screen.getByRole('heading', {name:'Kept'})).toBeTruthy()
})

it.each([
  '# [Guide](#author-anchor)',
  '# **[Guide](#author-anchor)**',
  '# ![Guide](/author-image.svg)',
  '# <a id="author-anchor"></a>Guide',
  '# Guide {#author-anchor}',
])('preserves title markup with authored actions, images or identifiers: %s', content => {
  render(<MarkdownContent content={content} frontmatter displayedTitle="Guide" />)
  expect(screen.getByRole('heading', {level:1})).toBeTruthy()
  if (content.includes('](#author-anchor)')) expect(screen.getByRole('link', {name:'Guide'}).getAttribute('href')).toBe('#author-anchor')
  if (content.includes('![')) expect(screen.getByRole('button', {name:'Guide'}).getAttribute('src')).toBe('/author-image.svg')
})
