import { useEffect, useRef, useState } from "react";
import { NavLink, useLocation } from "react-router-dom";
import {
  BookOpen,
  Boxes,
  BrainCircuit,
  ChevronDown,
  CircleHelp,
  CloudUpload,
  Compass,
  FolderOpen,
  Globe,
  LayoutDashboard,
  ListChecks,
  Menu,
  Moon,
  Network,
  Search,
  Server,
  Sparkles,
  Sun,
  Workflow,
  X,
} from "lucide-react";
import { useAppStore } from "@/store/appStore";
import { useTheme } from "@/hooks/useTheme";
import { agentFeaturesEnabled, cn } from "@/lib/utils";
import { TYPE_FILTERS } from "./Sidebar.constants";

export function Sidebar({ onClose }: { onClose?: () => void }) {
  const { theme, toggle } = useTheme();
  const { webMode, typeFilter, setTypeFilter } = useAppStore();
  const location = useLocation();
  const groups = [
    {
      title: "Engineering",
      items: [
        { to: "/workspace", label: "Workspace", icon: LayoutDashboard },
        { to: "/task/sessions", label: "Sessions", icon: Workflow },
        { to: "/task/explorer", label: "Tasks & evidence", icon: ListChecks },
        ...(agentFeaturesEnabled()
          ? [{ to: "/live", label: "Live search", icon: Search }]
          : []),
      ],
    },
    {
      title: "Project context",
      items: [
        { to: "/ast/explorer", label: "Code intelligence", icon: Network },
        { to: "/knowledge/explorer", label: "Knowledge", icon: BookOpen },
        {
          to: "/memory/explorer/project",
          label: "Project memory",
          icon: BrainCircuit,
        },
        {
          to: "/memory/explorer/user",
          label: "Personal memory",
          icon: BrainCircuit,
        },
      ],
    },
    {
      title: "Shared ecosystem",
      items: [
        { to: "/hub/registry", label: "Hub registry", icon: Compass },
        ...(!webMode
          ? [{ to: "/hub/local", label: "Project artifacts", icon: FolderOpen }]
          : []),
        { to: "/hub/upload", label: "Publish artifacts", icon: CloudUpload },
        { to: "/ast/contexts", label: "Code contexts", icon: Boxes },
        {
          to: "/knowledge/contexts",
          label: "Knowledge contexts",
          icon: BookOpen,
        },
      ],
    },
    {
      title: "Operations",
      items: [
        { to: "/system/ecosystem", label: "Projects", icon: Globe },
        { to: "/system/daemon", label: "Daemon", icon: Server },
        { to: "/system/dream", label: "Dream", icon: Sparkles },
      ],
    },
  ];
  return (
    <div className="app-sidebar">
      <NavLink
        to="/workspace"
        className="product-brand"
        onClick={onClose}
        aria-label="Graphit workspace"
      >
        <span className="brand-glyph" aria-hidden="true" />
        <span>
          <strong>Graphit</strong>
          <small>Engineering workspace</small>
        </span>
      </NavLink>
      <nav aria-label="Main navigation" className="product-nav">
        {groups.map((group) => (
          <section key={group.title} className="nav-group">
            <h2>{group.title}</h2>
            {group.items.map(({ to, label, icon: Icon }) => (
              <NavLink
                key={to}
                to={to}
                onClick={onClose}
                className={({ isActive }) =>
                  cn("nav-link", isActive && "is-active")
                }
              >
                <Icon size={17} strokeWidth={1.7} />
                <span>{label}</span>
              </NavLink>
            ))}
          </section>
        ))}
        {location.pathname.startsWith("/hub") && (
          <details className="nav-filter">
            <summary>
              Artifact type <ChevronDown size={14} />
            </summary>
            <div>
              {TYPE_FILTERS.map((f) => (
                <button
                  key={f.value}
                  aria-pressed={typeFilter === f.value}
                  onClick={() => setTypeFilter(f.value)}
                >
                  {f.icon}
                  {f.label}
                </button>
              ))}
            </div>
          </details>
        )}
      </nav>
      <div className="nav-footer">
        <a
          href="https://github.com/graphit-labs/graphit-code/tree/main/docs"
          target="_blank"
          rel="noreferrer"
        >
          <CircleHelp size={16} /> Documentation
        </a>
        <button onClick={toggle}>
          {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}{" "}
          {theme === "dark" ? "Light mode" : "Dark mode"}
        </button>
      </div>
    </div>
  );
}

export function MobileSidebar() {
  const [open, setOpen] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (open) dialog.current?.showModal();
    else if (dialog.current?.open) dialog.current.close();
  }, [open]);
  return (
    <>
      <button
        ref={trigger}
        className="mobile-nav-trigger"
        onClick={() => setOpen(true)}
        aria-label="Open navigation"
      >
        <Menu size={20} />
      </button>
      <dialog
        ref={dialog}
        className="mobile-nav-dialog"
        onCancel={() => setOpen(false)}
        onClose={() => {
          setOpen(false);
          trigger.current?.focus();
        }}
        onClick={(e) => {
          if (e.target === e.currentTarget) setOpen(false);
        }}
      >
        <button
          className="mobile-nav-close"
          onClick={() => setOpen(false)}
          aria-label="Close navigation"
        >
          <X size={18} />
        </button>
        <Sidebar onClose={() => setOpen(false)} />
      </dialog>
    </>
  );
}
