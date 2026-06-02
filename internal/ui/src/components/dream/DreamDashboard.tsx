import { useCallback, useEffect, useState } from 'react'
import { dreamApi, type DreamStatus, type DreamReport, type DreamSubject } from '@/api/dream'
import { useAppStore } from '@/store/appStore'
import {
  Sparkles,
  Plus,
  Trash2,
  FileText,
  Clock,
  Settings,
  RefreshCw,
  Moon,
  CheckCircle2,
  Calendar,
  AlertCircle,
  Inbox,
  Check,
  X,
} from 'lucide-react'
import { showToast } from '@/hooks/useToast'
import { WikiMarkdown } from '../wiki/WikiMarkdown'

export default function DreamDashboard() {
  const { activeProjectDir } = useAppStore()
  const [status, setStatus] = useState<DreamStatus | null>(null)
  const [reports, setReports] = useState<DreamReport[]>([])
  const [subjects, setSubjects] = useState<DreamSubject[]>([])
  const [loading, setLoading] = useState(true)

  // Subject adding form state
  const [newTitle, setNewTitle] = useState('')
  const [newBody, setNewBody] = useState('')
  const [addingSubject, setAddingSubject] = useState(false)
  const [showAddForm, setShowAddForm] = useState(false)

  // Selected item states
  const [selectedSubject, setSelectedSubject] = useState<DreamSubject | null>(null)
  const [selectedReport, setSelectedReport] = useState<DreamReport | null>(null)
  const [reportContent, setReportContent] = useState<string>('')
  const [loadingReport, setLoadingReport] = useState(false)

  const fetchData = useCallback(async (silent = false) => {
    if (!activeProjectDir) return
    if (!silent) setLoading(true)
    try {
      const [statusData, reportsData, subjectsData] = await Promise.all([
        dreamApi.getStatus(activeProjectDir),
        dreamApi.getReports(activeProjectDir),
        dreamApi.getSubjects(activeProjectDir),
      ])
      setStatus(statusData)
      setReports(reportsData || [])
      setSubjects(subjectsData || [])
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      showToast(`Failed to load dream data: ${msg}`, 'error')
    } finally {
      if (!silent) setLoading(false)
    }
  }, [activeProjectDir])

  useEffect(() => {
    const timer = setTimeout(() => {
      fetchData()
    }, 0)
    const interval = setInterval(() => fetchData(true), 10000)
    return () => {
      clearTimeout(timer)
      clearInterval(interval)
    }
  }, [activeProjectDir, fetchData])

  const handleAddSubject = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!newTitle.trim()) {
      showToast('Subject title is required', 'error')
      return
    }
    setAddingSubject(true)
    try {
      await dreamApi.addSubject(activeProjectDir, newTitle, newBody)
      showToast('Dream subject added successfully', 'success')
      setNewTitle('')
      setNewBody('')
      setShowAddForm(false)
      fetchData(true)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      showToast(`Failed to add subject: ${msg}`, 'error')
    } finally {
      setAddingSubject(false)
    }
  }

  const handleRemoveSubject = async (slug: string) => {
    if (!window.confirm('Are you sure you want to remove this subject?')) return
    try {
      await dreamApi.removeSubject(activeProjectDir, slug)
      showToast('Subject removed successfully', 'success')
      if (selectedSubject?.Slug === slug) {
        setSelectedSubject(null)
        setReportContent('')
      }
      fetchData(true)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      showToast(`Failed to remove subject: ${msg}`, 'error')
    }
  }

  const viewSubjectReport = async (sub: DreamSubject) => {
    setSelectedReport(null)
    setSelectedSubject(sub)
    setLoadingReport(true)
    setReportContent('')
    try {
      const dir = `${activeProjectDir}/.graphit/dream/subjects`
      const path = `${sub.Slug}.done.md`
      const url = `${useAppStore.getState().apiBase}/api/wiki/page?dir=${encodeURIComponent(dir)}&path=${encodeURIComponent(path)}`
      const res = await fetch(url)
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const data = await res.json()
      setReportContent(data.content || '')
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      showToast(`Failed to load resolution report: ${msg}`, 'error')
    } finally {
      setLoadingReport(false)
    }
  }

  const viewGeneralReport = async (report: DreamReport) => {
    setSelectedSubject(null)
    setSelectedReport(report)
    setLoadingReport(true)
    setReportContent('')
    try {
      const dir = `${activeProjectDir}/.graphit/dream`
      const path = `${report.id}.md`
      const url = `${useAppStore.getState().apiBase}/api/wiki/page?dir=${encodeURIComponent(dir)}&path=${encodeURIComponent(path)}`
      const res = await fetch(url)
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const data = await res.json()
      setReportContent(data.content || '')
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      showToast(`Failed to load report content: ${msg}`, 'error')
    } finally {
      setLoadingReport(false)
    }
  }

  if (loading && !status) {
    return (
      <div className="flex items-center justify-center min-h-[400px]">
        <div className="flex flex-col items-center gap-3">
          <RefreshCw className="w-8 h-8 text-primary animate-spin" />
          <p className="text-muted-foreground text-sm">Loading Dream Dashboard...</p>
        </div>
      </div>
    )
  }

  const statusColors: Record<string, string> = {
    dreaming: 'bg-primary/10 text-primary border-primary/20',
    'deep sleep': 'bg-purple-500/10 text-purple-400 border-purple-500/20',
    standby: 'bg-success/10 text-success border-success/20',
    inactive: 'bg-muted/10 text-muted-foreground border-muted/20',
  }

  const statusIcons: Record<string, React.ReactNode> = {
    dreaming: <Sparkles className="w-5 h-5 animate-pulse" />,
    'deep sleep': <Moon className="w-5 h-5" />,
    standby: <CheckCircle2 className="w-5 h-5" />,
    inactive: <AlertCircle className="w-5 h-5" />,
  }

  const renderPreview = () => {
    if (loadingReport) {
      return (
        <div className="flex items-center justify-center h-full min-h-[450px]">
          <RefreshCw className="w-8 h-8 animate-spin text-primary" />
        </div>
      )
    }

    if (selectedSubject) {
      if (selectedSubject.Done) {
        return (
          <div className="glass-panel rounded-2xl overflow-hidden flex flex-col h-[650px] shadow-sm">
            <div className="px-5 py-4 border-b border-border/40 bg-card/40 flex items-center justify-between">
              <div className="flex flex-col min-w-0">
                <span className="font-heading font-semibold text-sm truncate">{selectedSubject.Title}</span>
                <span className="text-[10px] text-muted-foreground">Task Resolution Report</span>
              </div>
              <span className="bg-success/15 text-success text-[9px] font-extrabold uppercase px-1.5 py-0.5 rounded border border-success/20">
                Completed
              </span>
            </div>
            <div className="flex-1 p-6 overflow-y-auto scrollbar-thin bg-card/15">
              <WikiMarkdown content={reportContent} />
            </div>
          </div>
        )
      } else {
        return (
          <div className="glass-panel rounded-2xl p-6 md:p-8 space-y-6 h-[400px] shadow-sm">
            <div className="flex items-center justify-between border-b border-border/40 pb-4">
              <h3 className="font-heading font-bold text-lg text-foreground">{selectedSubject.Title}</h3>
              <span className="bg-amber-500/10 text-amber-400 text-[10px] font-extrabold uppercase px-2 py-0.5 rounded border border-amber-500/20">
                Pending in Queue
              </span>
            </div>
            <div className="space-y-4">
              <p className="text-xs text-muted-foreground leading-relaxed">
                This task is currently queued. The background daemon will process it during the next idle period.
              </p>
              {selectedSubject.Body && (
                <div className="space-y-1.5">
                  <span className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Instructions:</span>
                  <pre className="bg-black/15 p-4 rounded-xl font-mono text-xs text-foreground/80 whitespace-pre-wrap max-h-48 overflow-y-auto leading-relaxed border border-border/30">
                    {selectedSubject.Body}
                  </pre>
                </div>
              )}
            </div>
          </div>
        )
      }
    }

    if (selectedReport) {
      return (
        <div className="glass-panel rounded-2xl overflow-hidden flex flex-col h-[650px] shadow-sm">
          <div className="px-5 py-4 border-b border-border/40 bg-card/40 flex items-center justify-between">
            <div className="flex flex-col min-w-0">
              <span className="font-heading font-semibold text-sm truncate">{selectedReport.title || 'General Exploration'}</span>
              <span className="text-[10px] text-muted-foreground">Session ID: {selectedReport.id}</span>
            </div>
            {selectedReport.has_deep_sleep && (
              <span className="bg-purple-500/10 text-purple-400 text-[9px] font-extrabold uppercase px-1.5 py-0.5 rounded border border-purple-500/20">
                Deep Sleep
              </span>
            )}
          </div>
          <div className="flex-1 p-6 overflow-y-auto scrollbar-thin bg-card/15">
            <WikiMarkdown content={reportContent} />
          </div>
        </div>
      )
    }

    return (
      <div className="glass-panel rounded-2xl p-8 flex flex-col items-center justify-center text-center h-[350px] text-muted-foreground gap-3 shadow-sm">
        <FileText className="w-10 h-10 opacity-30" />
        <div>
          <h4 className="font-bold text-foreground text-sm">Select a task or audit</h4>
          <p className="text-xs mt-1">Select a queued subject or general audit report from the sidebar list to view details.</p>
        </div>
      </div>
    )
  }

  return (
    <div className="w-full max-w-7xl mx-auto px-4 md:px-8 py-10 animate-in fade-in duration-300">
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 mb-8 pb-6 border-b border-border/40">
        <div className="flex items-center gap-4">
          <div className="w-12 h-12 rounded-2xl bg-primary/10 border border-primary/20 flex items-center justify-center shrink-0">
            <Sparkles className="w-6 h-6 text-primary" />
          </div>
          <div>
            <h1 className="text-3xl font-heading font-bold tracking-tight text-foreground">Dream</h1>
            <p className="text-[14px] text-muted-foreground mt-1">
              Queue instructions for the autonomous AI developer, run while you sleep
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2.5">
          <button
            onClick={() => fetchData()}
            className="flex items-center justify-center p-2.5 rounded-xl border border-border/50 hover:bg-accent/50 transition-all hover:scale-[1.02] glass-panel"
            title="Refresh"
          >
            <RefreshCw className="w-4 h-4 text-muted-foreground hover:rotate-180 transition-transform duration-500" />
          </button>
        </div>
      </div>

      {/* Stats Cards */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-5 mb-8">
        <div className="glass-panel p-5 rounded-2xl flex items-start gap-4 col-span-1 shadow-sm">
          <div className={`p-3 rounded-xl border ${statusColors[status?.status ?? 'inactive']}`}>
            {statusIcons[status?.status ?? 'inactive']}
          </div>
          <div className="space-y-1">
            <p className="text-xs text-muted-foreground font-semibold uppercase tracking-wider">Dream State</p>
            <h3 className="text-lg font-bold font-heading capitalize">{status?.status ?? 'inactive'}</h3>
            <span className="text-[10px] text-muted-foreground block">
              {status?.enabled ? 'Enabled in config' : 'Disabled'}
            </span>
          </div>
        </div>

        <div className="glass-panel p-5 rounded-2xl flex items-start gap-4 col-span-1 shadow-sm">
          <div className="p-3 rounded-xl bg-info/10 text-info">
            <Clock className="w-5 h-5" />
          </div>
          <div className="space-y-1">
            <p className="text-xs text-muted-foreground font-semibold uppercase tracking-wider">Idle Timeout</p>
            <h3 className="text-lg font-bold font-heading">{status?.idle_timeout}</h3>
            <p className="text-[10px] text-muted-foreground">
              Required inactivity time
            </p>
          </div>
        </div>

        <div className="glass-panel p-5 rounded-2xl flex items-start gap-4 col-span-1 shadow-sm">
          <div className="p-3 rounded-xl bg-primary/10 text-primary">
            <Settings className="w-5 h-5" />
          </div>
          <div className="space-y-1">
            <p className="text-xs text-muted-foreground font-semibold uppercase tracking-wider">Max Duration</p>
            <h3 className="text-lg font-bold font-heading">{status?.max_duration}</h3>
            <p className="text-[10px] text-muted-foreground">
              Time budget per session
            </p>
          </div>
        </div>

        <div className="glass-panel p-5 rounded-2xl flex items-start gap-4 col-span-1 shadow-sm">
          <div className="p-3 rounded-xl bg-success/10 text-success">
            <FileText className="w-5 h-5" />
          </div>
          <div className="space-y-1">
            <p className="text-xs text-muted-foreground font-semibold uppercase tracking-wider">Total Reports</p>
            <h3 className="text-lg font-bold font-heading">{status?.total_reports ?? 0}</h3>
            <p className="text-[10px] text-muted-foreground">
              Generated sleep sessions
            </p>
          </div>
        </div>
      </div>

      {/* Coupled layout */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Left column lists */}
        <div className="lg:col-span-1 flex flex-col gap-6">
          {/* Tasks & Subjects List */}
          <div className="glass-panel rounded-2xl overflow-hidden flex flex-col max-h-[480px] shadow-sm">
            <div className="px-5 py-4 border-b border-border/40 bg-card/40 flex items-center justify-between">
              <span className="font-heading font-semibold text-sm">Tasks & Subjects</span>
              <button
                onClick={() => setShowAddForm(true)}
                className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-xl bg-primary/10 border border-primary/20 text-primary text-xs font-semibold hover:bg-primary/20 hover:scale-[1.02] transition-all"
              >
                <Plus className="w-3.5 h-3.5" /> Queue Subject
              </button>
            </div>
            
            <div className="overflow-y-auto flex-1 divide-y divide-border/30 scrollbar-thin">
              {subjects.length > 0 ? (
                subjects.map((sub) => (
                  <button
                    key={sub.Slug}
                    onClick={() => {
                      if (sub.Done) {
                        viewSubjectReport(sub)
                      } else {
                        setSelectedReport(null)
                        setSelectedSubject(sub)
                      }
                    }}
                    className={`w-full text-left px-5 py-4 hover:bg-accent/30 transition-colors flex flex-col gap-2 ${
                      selectedSubject?.Slug === sub.Slug ? 'bg-primary/5' : ''
                    }`}
                  >
                    <div className="flex items-start justify-between gap-2 min-w-0">
                      <span className="font-heading font-bold text-sm truncate flex-1 leading-snug">
                        {sub.Title}
                      </span>
                      {sub.Done ? (
                        <span className="shrink-0 bg-success/15 text-success text-[9px] font-extrabold uppercase px-1.5 py-0.5 rounded border border-success/20 flex items-center gap-1">
                          <Check className="w-2.5 h-2.5" /> Done
                        </span>
                      ) : (
                        <span className="shrink-0 bg-amber-500/10 text-amber-400 text-[9px] font-extrabold uppercase px-1.5 py-0.5 rounded border border-amber-500/20">
                          Pending
                        </span>
                      )}
                    </div>
                    
                    <div className="flex items-center justify-between text-[10px] text-muted-foreground font-semibold mt-1">
                      <span>Created: {new Date(sub.CreatedAt).toLocaleDateString()}</span>
                      <button
                        onClick={(e) => {
                          e.stopPropagation()
                          handleRemoveSubject(sub.Slug)
                        }}
                        className="p-1 rounded hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-colors"
                        title="Remove subject"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </button>
                ))
              ) : (
                <div className="p-8 text-center text-muted-foreground flex flex-col items-center gap-2">
                  <Inbox className="w-8 h-8 opacity-40" />
                  <p className="text-xs">No dream subjects queued.</p>
                </div>
              )}
            </div>
          </div>

          {/* General Audits List */}
          <div className="glass-panel rounded-2xl overflow-hidden flex flex-col max-h-[350px] shadow-sm">
            <div className="px-5 py-4 border-b border-border/40 bg-card/40">
              <span className="font-heading font-semibold text-sm">General Audits</span>
            </div>
            <div className="overflow-y-auto flex-1 divide-y divide-border/30 scrollbar-thin">
              {reports.length > 0 ? (
                reports.map((report) => (
                  <button
                    key={report.id}
                    onClick={() => viewGeneralReport(report)}
                    className={`w-full text-left px-5 py-4 hover:bg-accent/30 transition-colors flex flex-col gap-2 ${
                      selectedReport?.id === report.id ? 'bg-primary/5' : ''
                    }`}
                  >
                    <div className="flex items-start justify-between gap-2 min-w-0">
                      <span className="font-heading font-bold text-sm truncate flex-1 leading-snug">
                        {report.title || 'Untitled Report'}
                      </span>
                      {report.has_deep_sleep && (
                        <span className="shrink-0 bg-purple-500/10 text-purple-400 text-[9px] font-extrabold uppercase px-1.5 py-0.5 rounded border border-purple-500/20">
                          Deep Sleep
                        </span>
                      )}
                    </div>
                    <div className="flex items-center gap-4 text-[10px] text-muted-foreground font-medium">
                      <span className="flex items-center gap-1">
                        <Calendar className="w-3.5 h-3.5" />
                        {new Date(report.created).toLocaleDateString()}
                      </span>
                      <span>{(report.size / 1024).toFixed(1)} KB</span>
                    </div>
                  </button>
                ))
              ) : (
                <div className="p-8 text-center text-muted-foreground flex flex-col items-center gap-2">
                  <Inbox className="w-8 h-8 opacity-40" />
                  <p className="text-xs">No general audits run yet.</p>
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Right column preview */}
        <div className="lg:col-span-2">
          {renderPreview()}
        </div>
      </div>

      {/* Subject Form Modal Overlay */}
      {showAddForm && (
        <div
          className="fixed inset-0 z-[9000] flex items-center justify-center backdrop-blur-md bg-black/40 animate-fade-in overflow-y-auto py-8"
          onClick={() => setShowAddForm(false)}
        >
          <div
            className="glass-panel bg-card/85 border border-border/50 rounded-2xl p-6 md:p-7 w-full max-w-lg shadow-2xl relative overflow-hidden my-auto space-y-4"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="absolute -top-12 -left-12 w-24 h-24 bg-primary/10 rounded-full blur-2xl pointer-events-none" />

            <div className="flex items-center justify-between mb-2 relative z-10">
              <h2 className="text-lg font-heading font-bold text-foreground">
                Queue Dream Subject
              </h2>
              <button
                onClick={() => setShowAddForm(false)}
                className="p-1.5 rounded-xl hover:bg-accent/50 text-muted-foreground hover:text-foreground transition-all"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
            
            <p className="text-sm text-muted-foreground mb-5 relative z-10 leading-relaxed">
              Define a specific objective or task context for the next autonomous dream execution.
            </p>

            <form onSubmit={handleAddSubject} className="relative z-10">
              <div className="mb-4 relative z-10">
                <label className="block text-xs font-semibold text-muted-foreground mb-1.5">Title</label>
                <input
                  type="text"
                  value={newTitle}
                  onChange={(e) => setNewTitle(e.target.value)}
                  placeholder="e.g. Add unit tests for git module"
                  className="w-full px-3.5 py-2.5 rounded-xl border border-border/50 bg-background/50 backdrop-blur-sm text-sm text-foreground outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary/80 transition-all duration-200"
                  required
                />
              </div>
              <div className="mb-4 relative z-10">
                <label className="block text-xs font-semibold text-muted-foreground mb-1.5">Instructions (Optional)</label>
                <textarea
                  value={newBody}
                  onChange={(e) => setNewBody(e.target.value)}
                  placeholder="Provide explicit context, files, or rules to enforce..."
                  rows={5}
                  className="w-full px-3.5 py-2.5 rounded-xl border border-border/50 bg-background/50 backdrop-blur-sm text-sm text-foreground outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary/80 transition-all duration-200 font-mono text-xs leading-relaxed"
                />
              </div>
              
              <div className="flex justify-end gap-2 relative z-10 pt-2">
                <button
                  type="button"
                  onClick={() => setShowAddForm(false)}
                  className="px-4 py-2 rounded-xl border border-border/50 text-sm font-semibold hover:bg-accent/40 transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={addingSubject}
                  className="px-4 py-2 rounded-xl text-sm font-semibold hover:scale-[1.01] transition-all btn-premium flex items-center gap-1.5"
                >
                  <Plus className="w-4 h-4" />
                  {addingSubject ? 'Adding...' : 'Queue Subject'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  )
}
