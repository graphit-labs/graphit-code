import { useLocation, useNavigate } from "react-router-dom";
import { WorkTabs } from "@/components/shared/EngineeringUI";
export type TaskExplorerMode = "tasks" | "gantt" | "sessions";
export function TaskModeToggle({ mode }: { mode: TaskExplorerMode }) {
  const navigate = useNavigate();
  const { search } = useLocation();
  return (
    <WorkTabs
      value={mode}
      onChange={(id) =>
        navigate((id === "tasks" ? "/task/explorer" : id === "gantt" ? "/task/gantt" : "/task/sessions") + search)
      }
      items={[
        ["tasks", "Tasks"],
        ["gantt", "Gantt"],
        ["sessions", "Sessions"],
      ]}
      label="Task explorer view"
    />
  );
}
