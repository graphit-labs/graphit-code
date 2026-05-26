import { Download, X } from 'lucide-react'
import { cn } from '@/lib/utils'

interface TabularResultsProps {
  columns: string[]
  rows: unknown[][]
  onClose: () => void
}

function downloadCSV(columns: string[], rows: unknown[][]) {
  const escape = (v: unknown) => {
    const s = typeof v === 'object' && v !== null ? JSON.stringify(v) : String(v ?? '')
    return `"${s.replace(/"/g, '""')}"`
  }
  const header = columns.map(escape).join(',')
  const body = rows.map((r) => r.map(escape).join(',')).join('\n')
  const blob = new Blob([header + '\n' + body], { type: 'text/csv;charset=utf-8;' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `query-results-${Date.now()}.csv`
  a.click()
  URL.revokeObjectURL(url)
}

export function TabularResults({ columns, rows, onClose }: TabularResultsProps) {
  return (
    <div className="absolute inset-x-0 bottom-0 z-20 bg-background/95 backdrop-blur-md border-t border-border/50 shadow-2xl max-h-[40%] flex flex-col rounded-t-2xl animate-in slide-in-from-bottom duration-300">
      {}
      <div className="flex items-center justify-between px-5 py-3 border-b border-border/40 bg-accent/15 shrink-0">
        <div className="flex items-center gap-2">
          <span className="w-1.5 h-1.5 rounded-full bg-primary" />
          <p className="text-xs font-bold text-foreground">
            Query Results — {rows.length} row{rows.length !== 1 ? 's' : ''}
          </p>
        </div>
        <div className="flex items-center gap-1.5">
          <button
            onClick={() => downloadCSV(columns, rows)}
            title="Download as CSV"
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold text-muted-foreground hover:text-foreground hover:bg-accent border border-transparent hover:border-border/30 transition-all duration-150"
          >
            <Download className="w-3.5 h-3.5" />
            Export CSV
          </button>
          <button
            onClick={onClose}
            className="p-1.5 rounded-xl hover:bg-accent border border-transparent hover:border-border/30 text-muted-foreground hover:text-foreground transition-all"
            title="Close results"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      </div>

      {}
      <div className="overflow-auto flex-1">
        <table className="w-full text-xs border-collapse">
          <thead className="sticky top-0 bg-muted/65 backdrop-blur-md z-10">
            <tr>
              {columns.map((col) => (
                <th key={col} className="text-left px-4 py-3 font-semibold text-muted-foreground/90 border-b border-border/40 whitespace-nowrap uppercase tracking-wider text-[10px]">
                  {col}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, i) => (
              <tr key={i} className="border-b border-border/20 hover:bg-accent/25 transition-colors">
                {row.map((cell, j) => (
                  <td key={j} className="px-4 py-2.5 text-muted-foreground font-mono max-w-xs truncate text-[12px]">
                    {typeof cell === 'object' && cell !== null
                      ? JSON.stringify(cell).slice(0, 120)
                      : String(cell ?? '')}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
