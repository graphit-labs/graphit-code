import { useLayoutEffect, useRef } from "react";
import { Link, useLocation } from "react-router-dom";
export type TaskExplorerMode = "tasks" | "gantt" | "sessions";
const views = [
  ["tasks", "Tasks", "/task/explorer"],
  ["gantt", "Gantt", "/task/gantt"],
  ["sessions", "Sessions", "/task/sessions"],
] as const;

export function TaskModeToggle({ mode }: { mode: TaskExplorerMode }) {
  const { search, state, key } = useLocation();
  const currentLink = useRef<HTMLAnchorElement>(null);
  const requestedMode = state?.taskModeTarget;
  useLayoutEffect(() => {
    if (requestedMode === mode) currentLink.current?.focus({ preventScroll: true });
  }, [key, mode, requestedMode]);
  const params = new URLSearchParams();
  for (const selection of new URLSearchParams(search).getAll("selection")) params.append("selection", selection);
  const scope = params.size ? "?" + params.toString() : "";
  return (
    <nav className="work-tabs" aria-label="Task explorer view">
      {views.map(([id, label, path]) => (
        <Link key={id} className="task-mode-link" ref={mode === id ? currentLink : undefined}
          aria-current={mode === id ? "page" : undefined} to={path + scope} state={{ taskModeTarget: id }}>
          {label}
        </Link>
      ))}
    </nav>
  );
}
