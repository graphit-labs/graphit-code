import { useCallback, useEffect, useState } from 'react'
import { dreamApi, type DreamReport, type DreamStatus } from '@/api/dream'
import { useAppStore } from '@/store/appStore'
import {
  AlertCircle,
  Calendar,
  CheckCircle2,
  Clock,
  FileText,
  Inbox,
  Moon,
  RefreshCw,
  Settings,
  Sparkles,
} from 'lucide-react'
import { showToast } from '@/hooks/useToast'
import { WikiMarkdown } from '../wiki/WikiMarkdown'

export default function DreamDashboard() {
  const { activeProjectDir } = useAppStore()
  const [status, setStatus] = useState<DreamStatus | null>(null)
  const [reports, setReports] = useState<DreamReport[]>([])
  const [loading, setLoading] = useState(true)
  const [selectedReport, setSelectedReport] = useState<DreamReport | null>(null)
  const [reportContent, setReportContent] = useState('')
  const [loadingReport, setLoadingReport] = useState(false)

  const fetchData = useCallback(async (silent = false) => {
    if (!activeProjectDir) return
    if (!silent) setLoading(true)
    try {
      const [statusData, reportsData] = await Promise.all([
        dreamApi.getStatus(activeProjectDir),
        dreamApi.getReports(activeProjectDir),
      ])
      setStatus(statusData)
      setReports(reportsData || [])
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      showToast(`Failed to load dream data: ${msg}`, 'error')
    } finally {
      if (!silent) setLoading(false)
    }
  }, [activeProjectDir])

  useEffect(() => {
    const timer = setTimeout(() => fetchData(), 0)
    const interval = setInterval(() => fetchData(true), 10000)
    return () => {
      clearTimeout(timer)
      clearInterval(interval)
    }
  }, [activeProjectDir, fetchData])

  const viewReport = async (report: DreamReport) => {
    setSelectedReport(report)
    setLoadingReport(true)
    setReportContent('')
    try {
      const sep = report.path.lastIndexOf('/')
      const dir = report.path.slice(0, sep)
      const path = report.path.slice(sep + 1)
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

    if (selectedReport) {
      return (
        <div className="glass-panel rounded-2xl overflow-hidden flex flex-col h-[650px] shadow-sm">
          <div className="px-5 py-4 border-b border-border/40 bg-card/40 flex items-center justify-between">
            <div className="flex flex-col min-w-0">
              <span className="font-heading font-semibold text-sm truncate">
                {selectedReport.title || 'Knowledge Exploration'}
              </span>
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
          <h4 className="font-bold text-foreground text-sm">Select a session report</h4>
          <p className="text-xs mt-1">Review the knowledge, memories, and agent artifacts improved during a Dream session.</p>
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
              Autonomous knowledge mining and agent-artifact improvement while you sleep
            </p>
          </div>
        </div>

        <button
          onClick={() => fetchData()}
          className="flex items-center justify-center p-2.5 rounded-xl border border-border/50 hover:bg-accent/50 transition-all hover:scale-[1.02] glass-panel"
          title="Refresh"
        >
          <RefreshCw className="w-4 h-4 text-muted-foreground hover:rotate-180 transition-transform duration-500" />
        </button>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-4 gap-5 mb-8">
        <div className="glass-panel p-5 rounded-2xl flex items-start gap-4 shadow-sm">
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

        <div className="glass-panel p-5 rounded-2xl flex items-start gap-4 shadow-sm">
          <div className="p-3 rounded-xl bg-info/10 text-info">
            <Clock className="w-5 h-5" />
          </div>
          <div className="space-y-1">
            <p className="text-xs text-muted-foreground font-semibold uppercase tracking-wider">Idle Timeout</p>
            <h3 className="text-lg font-bold font-heading">{status?.idle_timeout}</h3>
            <p className="text-[10px] text-muted-foreground">Required inactivity time</p>
          </div>
        </div>

        <div className="glass-panel p-5 rounded-2xl flex items-start gap-4 shadow-sm">
          <div className="p-3 rounded-xl bg-primary/10 text-primary">
            <Settings className="w-5 h-5" />
          </div>
          <div className="space-y-1">
            <p className="text-xs text-muted-foreground font-semibold uppercase tracking-wider">Max Duration</p>
            <h3 className="text-lg font-bold font-heading">{status?.max_duration}</h3>
            <p className="text-[10px] text-muted-foreground">Time budget per session</p>
          </div>
        </div>

        <div className="glass-panel p-5 rounded-2xl flex items-start gap-4 shadow-sm">
          <div className="p-3 rounded-xl bg-success/10 text-success">
            <FileText className="w-5 h-5" />
          </div>
          <div className="space-y-1">
            <p className="text-xs text-muted-foreground font-semibold uppercase tracking-wider">Total Reports</p>
            <h3 className="text-lg font-bold font-heading">{status?.total_reports ?? 0}</h3>
            <p className="text-[10px] text-muted-foreground">Knowledge-improvement sessions</p>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="glass-panel rounded-2xl overflow-hidden flex flex-col max-h-[650px] shadow-sm">
          <div className="px-5 py-4 border-b border-border/40 bg-card/40">
            <span className="font-heading font-semibold text-sm">Session Reports</span>
          </div>
          <div className="overflow-y-auto flex-1 divide-y divide-border/30 scrollbar-thin">
            {reports.length > 0 ? (
              reports.map((report) => (
                <button
                  key={report.id}
                  onClick={() => viewReport(report)}
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
                <p className="text-xs">No Dream sessions completed yet.</p>
              </div>
            )}
          </div>
        </div>

        <div className="lg:col-span-2">{renderPreview()}</div>
      </div>
    </div>
  )
}
