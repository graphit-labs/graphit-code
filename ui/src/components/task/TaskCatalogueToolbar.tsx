import { useId, useState, type ReactNode } from 'react'

export function TaskCatalogueToolbar({ search, children }: { search: ReactNode; children: ReactNode }) {
  const [expanded, setExpanded] = useState(false)
  const id = useId()
  return <div className="work-toolbar task-workspace-toolbar">
    {search}
    <button type="button" className="work-button task-filter-toggle" aria-expanded={expanded} aria-controls={id}
      onClick={() => setExpanded(value => !value)}>Filters</button>
    <div id={id} className="task-filter-options" data-expanded={expanded}>{children}</div>
  </div>
}
