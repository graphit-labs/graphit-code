import { useState, useRef, useEffect } from 'react'
import { cn } from '@/lib/utils'
import { Download, Check, Settings, AlertCircle, Trash2, ArrowUpCircle, CloudUpload,
  BookOpen, Zap, Bot, Scale, Code2, Terminal, Server, Wand2, FileText, ShieldCheck,
  ChevronDown, Tag, Layers } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import type { RegistryEntry, InstalledArtifact } from '@/api/hub'

const TYPE_ICONS: Record<string, LucideIcon> = {
  knowledge: BookOpen,
  skill: Zap,
  agent: Bot,
  rule: Scale,
  ast: Code2,
  command: Terminal,
  mcp: Server,
  power: Wand2,
}

const TYPE_STYLES: Record<string, { bg: string; text: string; border: string; glow: string; gradient: string }> = {
  knowledge: {
    bg: 'bg-blue-500/10',
    text: 'text-blue-500 dark:text-blue-400',
    border: 'border-blue-500/20',
    glow: 'bg-blue-500',
    gradient: 'from-blue-500 to-indigo-500',
  },
  skill: {
    bg: 'bg-amber-500/10',
    text: 'text-amber-500 dark:text-amber-400',
    border: 'border-amber-500/20',
    glow: 'bg-amber-500',
    gradient: 'from-amber-500 to-orange-500',
  },
  agent: {
    bg: 'bg-purple-500/10',
    text: 'text-purple-500 dark:text-purple-400',
    border: 'border-purple-500/20',
    glow: 'bg-purple-500',
    gradient: 'from-purple-500 to-indigo-500',
  },
  rule: {
    bg: 'bg-emerald-500/10',
    text: 'text-emerald-500 dark:text-emerald-400',
    border: 'border-emerald-500/20',
    glow: 'bg-emerald-500',
    gradient: 'from-emerald-500 to-teal-500',
  },
  ast: {
    bg: 'bg-pink-500/10',
    text: 'text-pink-500 dark:text-pink-400',
    border: 'border-pink-500/20',
    glow: 'bg-pink-500',
    gradient: 'from-pink-500 to-rose-500',
  },
  command: {
    bg: 'bg-cyan-500/10',
    text: 'text-cyan-500 dark:text-cyan-400',
    border: 'border-cyan-500/20',
    glow: 'bg-cyan-500',
    gradient: 'from-cyan-500 to-blue-500',
  },
  mcp: {
    bg: 'bg-red-500/10',
    text: 'text-red-500 dark:text-red-400',
    border: 'border-red-500/20',
    glow: 'bg-red-500',
    gradient: 'from-red-500 to-rose-500',
  },
  power: {
    bg: 'bg-violet-500/10',
    text: 'text-violet-500 dark:text-violet-400',
    border: 'border-violet-500/20',
    glow: 'bg-violet-500',
    gradient: 'from-violet-500 to-fuchsia-500',
  },
}

export interface ArtifactCardProps {
  variant: 'registry' | 'project' | 'imported'
  entry?: RegistryEntry | null
  installedInfo?: InstalledArtifact | null
  webMode?: boolean
  activeProjectId?: string
  projectPath?: string
  onInstall?: (id: string, type: string, withAlias?: boolean, version?: string) => void
  onUninstall?: (entry: RegistryEntry, installed: InstalledArtifact) => void
  onUpdate?: (id: string, type: string) => void
  onRemove?: (art: InstalledArtifact) => void
  onUnpublish?: (id: string, type: string) => void
  onSubmit?: (art: InstalledArtifact) => void
  onUnlink?: (art: InstalledArtifact) => void
  clusterLabels?: Record<string, string>
}

export function ArtifactCard({
  variant,
  entry,
  installedInfo,
  webMode = false,
  activeProjectId = '',
  projectPath = '',
  onInstall,
  onUninstall,
  onUpdate,
  onRemove,
  onUnpublish,
  onSubmit,
  onUnlink,
  clusterLabels,
}: ArtifactCardProps) {
  const [selectedVersion, setSelectedVersion] = useState('')
  const [versionOpen, setVersionOpen] = useState(false)
  const versionRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (versionRef.current && !versionRef.current.contains(e.target as Node)) {
        setVersionOpen(false)
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [])

  const displayName = variant === 'registry'
    ? entry?.name || ''
    : installedInfo?.registry_name || installedInfo?.local_id || ''

  const type = variant === 'registry'
    ? entry?.type || ''
    : installedInfo?.type || ''

  const description = variant === 'registry'
    ? entry?.description
    : installedInfo?.registry_description || 'No description available.'

  const tags = variant === 'registry'
    ? entry?.tags || []
    : installedInfo?.registry_tags || []

  const latestVersion = variant === 'registry'
    ? entry?.latest
    : installedInfo?.registry_version

  const installedVersion = installedInfo?.version

  const alias = installedInfo?.alias

  const authorName = variant === 'registry'
    ? entry?.author?.username ?? 'graphit'
    : installedInfo?.registry_author ?? 'graphit'

  const authorAvatar = variant === 'registry'
    ? entry?.author?.avatar_url ?? `https://avatars.githubusercontent.com/u/74853570?s=64&v=4`
    : `https://avatars.githubusercontent.com/u/74853570?s=64&v=4`

  const isInstalled = !!installedInfo
  const isManaged = variant === 'registry' && (
    installedInfo?.origin === 'managed' ||
    installedInfo?.origin === 'publish' ||
    (entry?.project_id != null && entry.project_id !== '' && entry.project_id === activeProjectId)
  )
  
  const hasUpdate = (variant === 'imported' || variant === 'project') && !!installedInfo?.has_update
  
  const canInstall = variant === 'registry' && !isManaged && !isInstalled

  const canUnpublish = variant === 'project' && installedInfo?.published === true

  const typeStyle = TYPE_STYLES[type] || {
    bg: 'bg-accent/30',
    text: 'text-accent-foreground',
    border: 'border-border/40',
    glow: 'bg-primary',
    gradient: 'from-primary to-blue-400',
  }

  const Icon = TYPE_ICONS[type] ?? FileText

  return (
    <div
      className={cn(
        'glass-panel rounded-2xl p-5 flex flex-col gap-4 relative overflow-hidden group',
        'transition-all duration-300 hover:border-primary/45 hover:shadow-xl hover:-translate-y-0.5',
        hasUpdate && 'border-orange-400/50',
      )}
    >
      {}
      <div className={cn(
        "absolute -right-8 -top-8 w-24 h-24 rounded-full blur-2xl opacity-0 group-hover:opacity-10 dark:group-hover:opacity-15 transition-all duration-500 scale-75 group-hover:scale-100 pointer-events-none z-0",
        typeStyle.glow
      )} />

      <div className="flex flex-col gap-2.5 relative z-10">
        <div className="flex items-start gap-3">
          <div className={cn("w-10 h-10 rounded-xl flex items-center justify-center shrink-0 shadow-inner bg-gradient-to-tr", typeStyle.gradient, "text-white p-0.5")}>
            <div className="w-full h-full rounded-[10px] bg-background flex items-center justify-center">
              <Icon className={cn("w-5 h-5", typeStyle.text)} />
            </div>
          </div>
          <div className="flex flex-col min-w-0 flex-1 justify-center min-h-10">
            <div className="flex items-center gap-2">
              <h3 className="font-heading font-semibold text-base text-foreground leading-tight truncate">
                {displayName}
              </h3>
              {hasUpdate && !webMode && (
                <span title="Update available"><AlertCircle className="w-4 h-4 text-orange-400 shrink-0" /></span>
              )}
            </div>
            {alias && alias !== (installedInfo?.local_id || '') && (
              <p className="text-[11px] text-muted-foreground truncate">alias: {alias}</p>
            )}
          </div>
        </div>

        {}
        <div className="flex flex-wrap gap-1.5 pl-13">
          <span className={cn("inline-flex items-center px-2 py-0.5 rounded-md text-[10px] font-semibold uppercase tracking-widest border", typeStyle.bg, typeStyle.text, typeStyle.border)}>
            {type}
          </span>
          {latestVersion && (
            <span className="inline-flex items-center px-2 py-0.5 rounded-md text-[10px] font-medium bg-muted/40 text-muted-foreground border border-border/50">
              v{latestVersion}
            </span>
          )}
          {variant === 'registry' && isManaged && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[10px] font-semibold bg-blue-500/10 text-blue-500 border border-blue-500/20">
              <ShieldCheck className="w-3 h-3" /> Managed
            </span>
          )}
          {variant === 'registry' && isInstalled && !isManaged && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[10px] font-semibold bg-emerald-500/10 text-emerald-500 border border-emerald-500/20">
              <Check className="w-3 h-3" /> Installed
            </span>
          )}
          {variant === 'project' && (
            <span className={cn(
              "px-2 py-0.5 rounded-md text-[10px] font-semibold border",
              installedInfo?.published
                ? 'bg-green-500/10 text-green-500 border-green-500/20'
                : 'bg-yellow-500/10 text-yellow-600 dark:text-yellow-400 border-yellow-500/20'
            )}>
              {installedInfo?.published ? 'published' : 'draft'}
            </span>
          )}
          {variant === 'imported' && (
            <span className="px-2 py-0.5 rounded-md text-[10px] font-semibold border bg-blue-500/10 text-blue-500 border-blue-500/20">
              imported
            </span>
          )}
          {}
          {clusterLabels && Object.keys(clusterLabels).length > 0 && ['knowledge', 'ast'].includes(type) && (
            Object.entries(clusterLabels).map(([k, v]) => (
              <span
                key={k}
                className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[10px] font-semibold bg-teal-500/10 text-teal-600 dark:text-teal-400 border border-teal-500/20"
              >
                <Layers className="w-3 h-3" />
                {k}={v}
              </span>
            ))
          )}
        </div>
      </div>

      {}
      <p className="text-[13px] text-muted-foreground/90 leading-relaxed flex-1 line-clamp-2 relative z-10 pl-13">
        {description}
      </p>

      {}
      {tags && tags.length > 0 && (
        <div className="flex flex-wrap gap-1.5 relative z-10 pl-13">
          {tags.slice(0, 4).map((tag) => (
            <span key={tag} className="text-[10px] px-1.5 py-0.5 rounded-md bg-muted/30 text-muted-foreground/80 border border-border/40 hover:bg-muted/50 transition-colors">
              {tag}
            </span>
          ))}
        </div>
      )}

      {}
      {(variant === 'project' || variant === 'imported') && installedInfo?.path && (
        <p className="text-[11px] text-muted-foreground/60 font-mono truncate pl-13 relative z-10">
          {projectPath && installedInfo.path.startsWith(projectPath)
            ? installedInfo.path.slice(projectPath.length).replace(/^\//, '')
            : installedInfo.path}
        </p>
      )}

      {}
      {variant === 'imported' && installedInfo && installedInfo.version && (
        <div className="pl-13 flex items-center gap-2">
          <span className="text-xs text-muted-foreground">
            v{installedInfo.version}
            {alias && <span className="text-muted-foreground/60"> [{alias}]</span>}
          </span>
          {hasUpdate && installedInfo.registry_version && (
            <span className="text-xs text-orange-500 dark:text-orange-400 font-medium">
              → v{installedInfo.registry_version} available
            </span>
          )}
        </div>
      )}

      {}
      {canInstall && entry && (
        <div className="pl-13 flex flex-col gap-1.5">
          {}
          {entry.versions && entry.versions.length > 1 && (
            <div ref={versionRef} className="relative">
              <button
                onClick={() => setVersionOpen((v) => !v)}
                className="flex items-center gap-2 w-full px-2.5 py-1.5 rounded-xl bg-accent/25 hover:bg-accent/40 border border-border/30 transition-all duration-200 group"
              >
                <Tag className="w-3 h-3 text-primary/70 shrink-0" />
                <span className="text-[11px] font-medium text-foreground flex-1 text-left truncate">
                  {selectedVersion ? `v${selectedVersion}` : `Latest (v${entry.latest})`}
                </span>
                <ChevronDown className={cn(
                  'w-3 h-3 text-muted-foreground/50 transition-transform duration-200 shrink-0',
                  versionOpen && 'rotate-180',
                )} />
              </button>

              {versionOpen && (
                <div className="absolute left-0 right-0 top-full mt-1 z-50 bg-card border border-border/50 rounded-xl shadow-2xl overflow-hidden animate-in fade-in slide-in-from-top-2 duration-150 max-h-[200px] overflow-y-auto scrollbar-none">
                  {}
                  <button
                    onClick={() => { setSelectedVersion(''); setVersionOpen(false) }}
                    className={cn(
                      'flex items-center gap-2.5 w-full px-3 py-2 text-left transition-all duration-150 hover:bg-accent/40',
                      !selectedVersion && 'bg-primary/8',
                    )}
                  >
                    <div className={cn(
                      'w-1.5 h-1.5 rounded-full shrink-0 transition-colors',
                      !selectedVersion ? 'bg-primary shadow-sm shadow-primary/30' : 'bg-muted-foreground/20',
                    )} />
                    <span className="text-[11px] font-medium text-foreground flex-1">Latest (v{entry.latest})</span>
                    {!selectedVersion && <Check className="w-3 h-3 text-primary shrink-0" />}
                  </button>
                  {}
                  {entry.versions.map((v) => (
                    <button
                      key={v}
                      onClick={() => { setSelectedVersion(v); setVersionOpen(false) }}
                      className={cn(
                        'flex items-center gap-2.5 w-full px-3 py-2 text-left transition-all duration-150 hover:bg-accent/40',
                        selectedVersion === v && 'bg-primary/8',
                      )}
                    >
                      <div className={cn(
                        'w-1.5 h-1.5 rounded-full shrink-0 transition-colors',
                        selectedVersion === v ? 'bg-primary shadow-sm shadow-primary/30' : 'bg-muted-foreground/20',
                      )} />
                      <span className="text-[11px] font-medium text-foreground flex-1">
                        v{v}{v === entry.latest ? ' (latest)' : ''}
                      </span>
                      {selectedVersion === v && <Check className="w-3 h-3 text-primary shrink-0" />}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {}
      {variant === 'registry' && isInstalled && !isManaged && installedVersion && (
        <div className="pl-13">
          <span className="text-xs text-muted-foreground">
            Installed: v{installedVersion}
            {alias && <span className="text-muted-foreground/60"> [{alias}]</span>}
          </span>
        </div>
      )}

      {}
      <div className="flex items-center justify-between pt-3 border-t border-border/40 mt-auto relative z-10">
        {}
        <div className="flex items-center gap-2">
          <img src={authorAvatar} alt={authorName} className="w-6 h-6 rounded-full shadow-sm border border-border/50" />
          <span className="text-xs text-muted-foreground font-medium">{authorName}</span>
        </div>

        {}
        <div className="flex items-center gap-1.5 opacity-80 group-hover:opacity-100 transition-opacity">
          {variant === 'registry' && (
            webMode ? (
              entry && (
                <a
                  href={`/api/download/${entry.id}?type=${entry.type}`}
                  className="flex items-center gap-1.5 px-4 py-1.5 rounded-xl border border-primary/20 text-[12px] font-semibold bg-primary/10 text-primary hover:bg-primary/20 hover:scale-[1.02] transition-all"
                >
                  <Download className="w-3.5 h-3.5" />
                  Download
                </a>
              )
            ) : (
              entry && (
                <>
                  {}
                  {!isInstalled && !isManaged && onInstall && (
                    <button
                      onClick={() => onInstall(entry.id, entry.type, true, selectedVersion || undefined)}
                      className="w-8 h-8 rounded-xl border border-border/50 text-muted-foreground bg-background/50 flex items-center justify-center hover:bg-accent/45 hover:text-foreground transition-all hover:scale-[1.02]"
                      title="Install with alias"
                    >
                      <Settings className="w-3.5 h-3.5" />
                    </button>
                  )}

                  {}
                  {isManaged ? (
                    <span
                      className="flex items-center gap-1.5 px-4 py-1.5 rounded-xl text-[12px] font-semibold bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20"
                      title="This artifact belongs to your project"
                    >
                      <ShieldCheck className="w-3.5 h-3.5" />
                      Managed
                    </span>
                  ) : installedInfo?.origin === 'link' && onUnlink ? (
                    <button
                      onClick={(e) => {
                        e.stopPropagation()
                        onUnlink(installedInfo)
                      }}
                      className={cn(
                        'flex items-center gap-1.5 px-4 py-1.5 rounded-xl text-[12px] font-semibold transition-all duration-200',
                        'bg-red-500/10 text-red-600 dark:text-red-400 border border-red-500/20 hover:bg-red-500/20 hover:scale-[1.02]'
                      )}
                      title="Click to unlink"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                      Unlink
                    </button>
                  ) : (
                    <div className="flex items-center gap-1.5">
                      {}
                      {onUninstall && onInstall && (
                        <button
                          onClick={() =>
                            isInstalled
                              ? onUninstall(entry!, installedInfo!)
                              : onInstall(entry!.id, entry!.type, false, selectedVersion || undefined)
                          }
                          className={cn(
                            'flex items-center gap-1.5 px-4 py-1.5 rounded-xl text-[12px] font-semibold transition-all duration-200',
                            isInstalled
                              ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20 hover:bg-emerald-500/20 hover:scale-[1.02]'
                              : 'btn-premium',
                          )}
                          title={isInstalled ? 'Click to remove' : 'Install locally'}
                        >
                          {isInstalled ? (
                            <>
                              <Check className="w-3.5 h-3.5" />
                              Installed
                            </>
                          ) : (
                            <>
                              <Download className="w-3.5 h-3.5" />
                              Install
                            </>
                          )}
                        </button>
                      )}
                    </div>
                  )}
                </>
              )
            )
          )}

          {variant === 'project' && installedInfo && (
            <div className="flex items-center gap-1.5">
              {canUnpublish && onUnpublish && (
                <button
                  onClick={() => onUnpublish(installedInfo.local_id, installedInfo.type)}
                  className="w-8 h-8 rounded-xl border border-border/50 text-muted-foreground bg-background/50 flex items-center justify-center hover:bg-red-500/10 hover:text-red-500 transition-all hover:scale-[1.02]"
                  title="Unpublish from registry"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
              )}
              {onSubmit && (
                <button
                  onClick={() => onSubmit(installedInfo)}
                  className="flex items-center gap-1.5 px-4 py-1.5 rounded-xl text-[12px] font-semibold transition-all duration-200 btn-premium"
                >
                  {installedInfo.published ? (
                    <>
                      <ArrowUpCircle className="w-3.5 h-3.5" />
                      Update
                    </>
                  ) : (
                    <>
                      <CloudUpload className="w-3.5 h-3.5" />
                      Submit
                    </>
                  )}
                </button>
              )}
            </div>
          )}

          {variant === 'imported' && installedInfo && (
            <div className="flex items-center gap-1.5">
              {}
              {hasUpdate && onUpdate && (
                <button
                  onClick={() => onUpdate(installedInfo.remote_id || installedInfo.local_id, installedInfo.type)}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-[12px] font-semibold transition-all duration-200 bg-orange-500/10 text-orange-600 dark:text-orange-400 border border-orange-500/20 hover:bg-orange-500/20 hover:scale-[1.02]"
                  title={`Update to v${installedInfo.registry_version ?? 'latest'}`}
                >
                  <ArrowUpCircle className="w-3.5 h-3.5" />
                  Update
                </button>
              )}
              {}
              {onRemove && (
                <button
                  onClick={() => onRemove(installedInfo)}
                  className="flex items-center gap-1.5 px-4 py-1.5 rounded-xl text-[12px] font-semibold bg-red-500/10 text-red-600 dark:text-red-400 border border-red-500/20 hover:bg-red-500/20 hover:scale-[1.02] transition-all duration-200"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                  Remove
                </button>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
