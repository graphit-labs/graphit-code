import { describe, expect, it } from 'vitest'
import { describeVersion, publishedForVersion } from './projectVersion'

describe('describeVersion', () => {
  it('says what a version is and holds in one sentence', () => {
    expect(describeVersion({ name: 'main', default: true, current: true, checkouts: ['/work/platform'], in_git: true, sources: ['checkout', 'task'] }, { remote: false, git: true }))
      .toBe('Default branch, checked out in platform, has tasks.')
    expect(describeVersion({ name: 'old', default: false, current: false, in_git: false, sources: ['ast', 'knowledge', 'memory'] }, { remote: false, git: true }))
      .toBe('Has code index, docs and memory, branch no longer in Git.')
    expect(describeVersion({ name: 'next', default: false, current: false, in_git: true, sources: [] }, { remote: false, git: true }))
      .toBe('No Graphit data yet.')
    expect(describeVersion({ name: '', default: true, current: false, in_git: false, sources: [] }, { remote: true, git: false }))
      .toBe("The project's main line.")
  })
})

describe('publishedForVersion', () => {
  const entry = { id: 'api-ast', versions: ['branch/main', 'branch/feature/x', '1.0.0'], qualified_latest: 'api-ast@1.0.0' }
  it('reads the publication of the selected branch, else the latest', () => {
    expect(publishedForVersion(entry, 'feature/x')).toEqual({ context: 'api-ast@branch/feature/x', matched: true })
    expect(publishedForVersion(entry, 'feature/y')).toEqual({ context: 'api-ast@1.0.0', matched: false })
    expect(publishedForVersion(entry, '')).toEqual({ context: 'api-ast@1.0.0', matched: true })
  })
})
