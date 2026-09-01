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
          'group relative flex items-center gap-3 px-3 py-2 rounded-lg text-[13px] font-semibold transition-all duration-200',
          isActive
            ? 'bg-[#b9fb63] text-[#101311] shadow-[0_8px_22px_-14px_rgba(185,251,99,0.9)]'
            : 'text-white/56 hover:text-white hover:bg-white/[0.07]',
        )
      }
    >
      <span className="w-5 h-5 rounded-md shrink-0 flex items-center justify-center border border-current/10 bg-current/[0.04] transition-transform duration-200 group-hover:scale-105">{icon}</span>
      <span className="flex-1 truncate">{label}</span>
      {badge != null && badge > 0 && (
        <span className="ml-auto bg-[#101311] text-[#b9fb63] text-[10px] font-bold px-2 py-0.5 rounded-full">
          {badge}
        </span>
      )}
    </NavLink>
  )
}

function NavSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="mb-5">
      <p className="px-3 py-1 font-mono text-[9px] font-semibold uppercase tracking-[0.18em] text-white/28 mb-1.5">
        {title}
      </p>
      <div className="flex flex-col gap-0.5">{children}</div>
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
          'group relative flex items-center gap-3 p-3 rounded-xl border transition-all duration-200 overflow-hidden',
          isActive
            ? 'bg-[#b9fb63] border-[#b9fb63] text-[#101311] shadow-[0_14px_32px_-18px_rgba(185,251,99,0.8)]'
            : 'bg-[#b9fb63]/[0.07] border-[#b9fb63]/20 text-white hover:bg-[#b9fb63]/[0.12] hover:border-[#b9fb63]/40',
        )
      }
    >
      <span className="absolute -top-7 -right-5 w-20 h-20 rounded-full border border-current/10 pointer-events-none" />
      <span className="w-8 h-8 shrink-0 rounded-lg border border-current/20 bg-current/[0.06] flex items-center justify-center transition-transform duration-200 group-hover:scale-105">
        {icon}
      </span>
      <span className="flex flex-col min-w-0 relative z-10">
        <span className="text-sm font-extrabold tracking-tight truncate">{label}</span>
        {hint && (
          <span className="text-[10px] opacity-55 truncate leading-tight font-medium">{hint}</span>
        )}
      </span>
    </NavLink>
  )
}

export const TYPE_FILTERS = [
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
    <div className="app-sidebar flex flex-col h-full relative overflow-hidden">
      <div className="px-5 pt-5 pb-4 border-b border-white/[0.08] relative z-10">
        <div className="flex items-center gap-3 font-heading font-bold text-base relative z-10">
          <div className="brand-glyph" aria-hidden="true" />
          <div className="flex flex-col min-w-0">
            <span className="font-extrabold tracking-[-0.04em] text-[17px] leading-tight truncate text-white">
              Graphit
            </span>
            <span className="font-mono text-[8px] text-[#b9fb63]/70 uppercase tracking-[0.18em] font-semibold leading-none mt-1">
              Code intelligence
            </span>
          </div>
          <span className="ml-auto self-start mt-0.5 font-mono text-[8px] uppercase tracking-wider text-white/25">OS</span>
        </div>
      </div>

      <ProjectSwitcher />

      <nav className="flex-1 overflow-y-auto px-3.5 py-4 space-y-1 scrollbar-none relative z-10">
        <div className="mb-5">
          <PrimaryNavItem
            to="/live"
            icon={<Search className="w-4 h-4" />}
            label="Live Search"
            hint="Search across every signal"
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

        {isRegistry && (
          <>
            <div className="border-t border-white/[0.08] my-4" />
            <div className="mb-4">
              <button
                className="flex items-center justify-between w-full px-3 py-1.5 font-mono text-[9px] font-semibold uppercase tracking-[0.18em] text-white/30 hover:text-white transition-colors"
                onClick={() => setShowTypeFilters((v) => !v)}
                aria-expanded={showTypeFilters}
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
                <div className="grid grid-cols-2 gap-1 mt-2">
                  {TYPE_FILTERS.map((f) => (
                    <button
                      key={f.value}
                      onClick={() => {
                        setTypeFilter(f.value)
                        close()
                      }}
                      className={cn(
                        'flex items-center gap-2 px-2.5 py-2 rounded-lg text-[11px] text-left transition-all duration-200',
                        typeFilter === f.value
                          ? 'bg-white text-[#101311] font-bold'
                          : 'text-white/48 hover:text-white hover:bg-white/[0.07]',
                      )}
                    >
                      <span className="w-4 shrink-0 flex items-center justify-center opacity-60">
                        {f.icon}
                      </span>
                      <span className="truncate">{f.label}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          </>
        )}
      </nav>

      <div className="px-3.5 pb-3.5 border-t border-white/[0.08] pt-3 flex flex-col gap-1 relative z-10">
        <button
          onClick={toggle}
          className="flex items-center gap-2.5 w-full px-3 py-2.5 rounded-lg text-xs font-semibold text-white/48 hover:text-white hover:bg-white/[0.07] transition-all duration-200"
        >
          {theme === 'dark' ? <Sun className="w-4 h-4 text-[#ffd56a]" /> : <Moon className="w-4 h-4 text-[#b9fb63]" />}
          <span>{theme === 'dark' ? 'Light mode' : 'Dark mode'}</span>
          <span className="ml-auto font-mono text-[8px] uppercase tracking-wider text-white/20">Theme</span>
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
        className="fixed top-3 left-3 z-50 md:hidden bg-[#161a18] text-[#b9fb63] border border-white/10 rounded-lg p-2.5 shadow-xl hover:scale-105 transition-all duration-200"
        onClick={() => setOpen(true)}
        aria-label="Open navigation"
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
                className="absolute top-4 right-4 z-50 p-2 rounded-lg bg-white/[0.06] border border-white/10 hover:bg-white/10 text-white/55 hover:text-white transition-all duration-200"
                onClick={() => setOpen(false)}
                aria-label="Close navigation"
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
