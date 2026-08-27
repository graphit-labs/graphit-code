import { useState, useEffect } from 'react'
import { NavLink, useLocation } from 'react-router-dom'
import { cn } from '@/lib/utils'
import { useTheme } from '@/hooks/useTheme'
import { useAppStore } from '@/store/appStore'
import { ProjectSwitcher } from './ProjectSwitcher'
import { fetchModules, type WikiModule } from '@/api/wiki'
import {
  Layers,
  Compass,
  FolderOpen,
  CloudUpload,
  Network,
  Sun,
  Moon,
  Menu,
  X,
  Globe,
  ChevronRight,
  ChevronDown,
  Code2,
  Database,
  Scale,
  Bot,
  Terminal,
  Server,
  Wand2,
  BookOpen,
  User,
  FolderGit2,
  Search,
  Sparkles,
  FileCode2,
  Blocks,
} from 'lucide-react'

interface NavItemProps {
  to: string
  icon: React.ReactNode
  label: string
  badge?: number | null
  end?: boolean
  onClick?: () => void
}

function NavItem({ to, icon, label, badge, end, onClick }: NavItemProps) {
  return (
    <NavLink
      to={to}
      end={end}
      onClick={onClick}
      className={({ isActive }) =>
        cn(
          'relative flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-sm font-medium transition-all duration-300',
          isActive
            ? 'bg-primary/10 text-primary border-l-2 border-primary pl-3 shadow-[inset_0_1px_0_0_rgba(255,255,255,0.03)]'
            : 'text-muted-foreground hover:text-foreground hover:bg-accent/40 hover:translate-x-0.5',
        )
      }
    >
      <span className="w-4 shrink-0 flex items-center justify-center transition-transform duration-300 group-hover:scale-110">{icon}</span>
      <span className="flex-1 truncate">{label}</span>
      {badge != null && badge > 0 && (
        <span className="ml-auto bg-primary text-primary-foreground text-[10px] font-bold px-2 py-0.5 rounded-full shadow-sm animate-pulse">
          {badge}
        </span>
      )}
    </NavLink>
  )
}

function NavSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="mb-6">
      <p className="px-3.5 py-1 text-[10px] font-bold uppercase tracking-widest text-muted-foreground/50 mb-2">
        {title}
      </p>
      <div className="flex flex-col gap-1">{children}</div>
    </div>
  )
}

/**
 * A single, visually louder entry that sits at the root of the navigation rather
 * than inside a section.
 *
 * Live Search runs an agent over any artifacts the user selects — documentation,
 * code graphs, rules, whatever the Hub carries — so filing it under one of them
 * misrepresented what it does and buried the entry point most sessions start from.
 */
function PrimaryNavItem({
  to, icon, label, hint, onClick,
}: {
  to: string
  icon: React.ReactNode
  label: string
  hint?: string
  onClick?: () => void
}) {
  return (
    <NavLink
      to={to}
      onClick={onClick}
      className={({ isActive }) =>
        cn(
          'group relative flex items-center gap-3 px-3.5 py-3 rounded-xl border transition-all duration-300 overflow-hidden',
          isActive
            ? 'bg-gradient-to-r from-primary/20 to-purple-500/15 border-primary/40 text-foreground shadow-[0_2px_12px_-4px_rgba(59,130,246,0.4)]'
            : 'bg-gradient-to-r from-primary/10 to-purple-500/[0.06] border-primary/20 text-foreground hover:border-primary/40 hover:from-primary/15 hover:to-purple-500/10',
        )
      }
    >
      <span className="absolute -top-6 -right-6 w-16 h-16 bg-primary/10 rounded-full blur-2xl pointer-events-none" />
      <span className="w-8 h-8 shrink-0 rounded-lg bg-gradient-to-br from-primary/25 to-purple-500/20 border border-primary/25 flex items-center justify-center text-primary transition-transform duration-300 group-hover:scale-105">
        {icon}
      </span>
      <span className="flex flex-col min-w-0 relative z-10">
        <span className="text-sm font-bold tracking-tight truncate">{label}</span>
        {hint && (
          <span className="text-[10px] text-muted-foreground/70 truncate leading-tight">{hint}</span>
        )}
      </span>
    </NavLink>
  )
}

const TYPE_FILTERS = [
  { label: 'Any Type', value: 'all', icon: <Layers className="w-3.5 h-3.5" /> },
  { label: 'Knowledge', value: 'knowledge', icon: <BookOpen className="w-3.5 h-3.5" /> },
  { label: 'Skill', value: 'skill', icon: <Wand2 className="w-3.5 h-3.5" /> },
  { label: 'Agent', value: 'agent', icon: <Bot className="w-3.5 h-3.5" /> },
  { label: 'Rule', value: 'rule', icon: <Scale className="w-3.5 h-3.5" /> },
  { label: 'AST', value: 'ast', icon: <Code2 className="w-3.5 h-3.5" /> },
  { label: 'Command', value: 'command', icon: <Terminal className="w-3.5 h-3.5" /> },
  { label: 'MCP Server', value: 'mcp', icon: <Server className="w-3.5 h-3.5" /> },
  { label: 'Power', value: 'power', icon: <Layers className="w-3.5 h-3.5" /> },
  { label: 'Language', value: 'language', icon: <FileCode2 className="w-3.5 h-3.5" /> },
  { label: 'Framework', value: 'framework', icon: <Blocks className="w-3.5 h-3.5" /> },
]

interface SidebarProps {
  onClose?: () => void
}

export function Sidebar({ onClose }: SidebarProps) {
  const location = useLocation()
  const { theme, toggle } = useTheme()
  const { typeFilter, setTypeFilter, webMode, projectName, activeProjectDir } =
    useAppStore()
  const isRegistry = location.pathname.startsWith('/hub')
  const [showTypeFilters, setShowTypeFilters] = useState(true)
  const pName = projectName || 'Project'

  const [memoryModules, setMemoryModules] = useState<WikiModule[]>([])
  useEffect(() => {
    fetchModules(activeProjectDir || undefined)
      .then(mods => setMemoryModules((mods ?? []).filter(m => m.id.startsWith('memory-'))))
      .catch(() => setMemoryModules([]))
  }, [activeProjectDir])

  useEffect(() => {
    const { loadProjects, projectsLoaded } = useAppStore.getState()
    if (!projectsLoaded) loadProjects()
  }, [])

  const close = () => onClose?.()

  return (
    <div className="flex flex-col h-full bg-card/60 backdrop-blur-2xl border-r border-border/40 relative">
      {}
      <div className="absolute -top-24 -left-24 w-48 h-48 bg-primary/5 rounded-full blur-3xl pointer-events-none" />

      <div className="px-5 py-5 border-b border-border/40 relative overflow-hidden">
        <div className="absolute -top-12 -left-12 w-24 h-24 bg-primary/10 rounded-full blur-2xl pointer-events-none" />
        
        <div className="flex items-center gap-3 font-heading font-bold text-base text-foreground relative z-10">
          <img
            src="/logo.svg"
            alt="Graphit Code Logo"
            className="w-10 h-10 object-contain shrink-0 filter drop-shadow-[0_2px_8px_rgba(59,130,246,0.25)] hover:scale-[1.05] transition-transform duration-300"
          />
          <div className="flex flex-col min-w-0">
            <span className="gradient-text font-bold tracking-tight text-[17px] leading-tight truncate">
              Graphit Code
            </span>
            <span className="text-[9px] text-muted-foreground/60 uppercase tracking-widest font-extrabold leading-none mt-0.5 font-sans">
              Knowledge Engine
            </span>
          </div>
        </div>
      </div>

      <ProjectSwitcher />

      <nav className="flex-1 overflow-y-auto px-4 py-6 space-y-2 scrollbar-none relative z-10">
        {}
        <div className="mb-6">
          <PrimaryNavItem
            to="/live"
            icon={<Search className="w-4 h-4" />}
            label="Live Search"
            hint="Any hub artifact, streamed"
            onClick={close}
          />
        </div>

        {}
        <NavSection title="Hub">
          <NavItem
            to="/hub/registry"
            icon={<Compass className="w-3.5 h-3.5" />}
            label="Registry"
            onClick={close}
          />
          {!webMode && (
            <NavItem
              to="/hub/local"
              icon={<FolderOpen className="w-3.5 h-3.5" />}
              label="Project Artifacts"
              onClick={close}
            />
          )}
          <NavItem
            to="/hub/upload"
            icon={<CloudUpload className="w-3.5 h-3.5" />}
            label="Upload"
            onClick={close}
          />
        </NavSection>

        {}
        <NavSection title="Knowledge">
          <NavItem
            to="/knowledge/contexts"
            icon={<Database className="w-3.5 h-3.5" />}
            label="All Contexts"
            onClick={close}
          />
          <NavItem
            to="/knowledge/explorer"
            icon={<BookOpen className="w-3.5 h-3.5" />}
            label={pName}
            onClick={close}
          />
        </NavSection>

        {}
        {
          <NavSection title="AST">
            <NavItem
              to="/ast/contexts"
              icon={<Database className="w-3.5 h-3.5" />}
              label="All Contexts"
              onClick={close}
            />
            <NavItem
              to="/ast/explorer"
              icon={<Network className="w-3.5 h-3.5" />}
              label={pName}
              onClick={close}
            />
          </NavSection>
        }

        {}
        {memoryModules.length > 0 && (
          <NavSection title="Memory">
            {memoryModules.map(mod => {
              const isUser = mod.context === 'user'
              return (
                <NavItem
                  key={mod.id}
                  to={`/memory/explorer/${mod.context}`}
                  icon={isUser
                    ? <User className="w-3.5 h-3.5" />
                    : <FolderGit2 className="w-3.5 h-3.5" />
                  }
                  label={isUser ? 'User' : pName}
                  onClick={close}
                />
              )
            })}
          </NavSection>
        )}

        {}
        <NavSection title="System">
          <NavItem
            to="/system/daemon"
            icon={<Server className="w-3.5 h-3.5" />}
            label="Daemon"
            onClick={close}
          />
          <NavItem
            to="/system/dream"
            icon={<Sparkles className="w-3.5 h-3.5" />}
            label="Dream"
            onClick={close}
          />
          <NavItem
            to="/system/ecosystem"
            icon={<Globe className="w-3.5 h-3.5" />}
            label="Ecosystem"
            onClick={close}
          />
        </NavSection>

        {}
        {isRegistry && (
          <>
            <div className="border-t border-border/30 my-4" />
            <div className="mb-4">
              <button
                className="flex items-center justify-between w-full px-3.5 py-1.5 text-[10px] font-bold uppercase tracking-widest text-muted-foreground/50 hover:text-foreground transition-colors"
                onClick={() => setShowTypeFilters((v) => !v)}
              >
                <span className="flex items-center gap-1.5">
                  Type Filter
                </span>
                {showTypeFilters ? (
                  <ChevronDown className="w-3 h-3" />
                ) : (
                  <ChevronRight className="w-3 h-3" />
                )}
              </button>
              {showTypeFilters && (
                <div className="flex flex-col gap-1 mt-2 pl-1.5 border-l border-border/30 ml-2">
                  {TYPE_FILTERS.map((f) => (
                    <button
                      key={f.value}
                      onClick={() => {
                        setTypeFilter(f.value)
                        close()
                      }}
                      className={cn(
                        'flex items-center gap-2.5 px-3 py-2 rounded-xl text-xs text-left transition-all duration-200',
                        typeFilter === f.value
                          ? 'bg-primary/10 text-primary font-semibold'
                          : 'text-muted-foreground hover:text-foreground hover:bg-accent/40',
                      )}
                    >
                      <span className="w-4 shrink-0 flex items-center justify-center text-muted-foreground/60">
                        {f.icon}
                      </span>
                      {f.label}
                    </button>
                  ))}
                </div>
              )}
            </div>
          </>
        )}
      </nav>

      {}
      <div className="px-4 pb-4 border-t border-border/40 pt-4 flex flex-col gap-1 relative z-10">
        <button
          onClick={toggle}
          className="flex items-center gap-2.5 w-full px-3.5 py-2.5 rounded-xl text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-accent/40 transition-all duration-200"
        >
          {theme === 'dark' ? <Sun className="w-4 h-4 text-amber-400" /> : <Moon className="w-4 h-4 text-blue-500" />}
          <span>{theme === 'dark' ? 'Light mode' : 'Dark mode'}</span>
        </button>
      </div>
    </div>
  )
}

export function MobileSidebar() {
  const [open, setOpen] = useState(false)
  return (
    <>
      <button
        className="fixed top-4 left-4 z-50 md:hidden bg-card/85 backdrop-blur border border-border/50 rounded-xl p-2.5 shadow-md hover:scale-105 transition-all duration-200"
        onClick={() => setOpen(true)}
      >
        <Menu className="w-4 h-4" />
      </button>

      {open && (
        <>
          <div
            className="fixed inset-0 z-40 bg-black/60 backdrop-blur-sm md:hidden"
            onClick={() => setOpen(false)}
          />
          <div className="fixed inset-y-0 left-0 z-50 w-72 md:hidden animate-in slide-in-from-left duration-300">
            <div className="h-full relative">
              <button
                className="absolute top-4 right-4 z-50 p-2 rounded-xl bg-card border border-border/50 hover:bg-accent text-muted-foreground hover:text-foreground transition-all duration-200"
                onClick={() => setOpen(false)}
              >
                <X className="w-3.5 h-3.5" />
              </button>
              <Sidebar onClose={() => setOpen(false)} />
            </div>
          </div>
        </>
      )}
    </>
  )
}
