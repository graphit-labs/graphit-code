import { useNavigate } from "react-router-dom";
import { WorkTabs } from "@/components/shared/EngineeringUI";
export type TaskExplorerMode = "tasks" | "sessions";
export function TaskModeToggle({ mode }: { mode: TaskExplorerMode }) {
  const navigate = useNavigate();
  return (
    <WorkTabs
      value={mode}
      onChange={(id) =>
        navigate(id === "tasks" ? "/task/explorer" : "/task/sessions")
      }
      items={[
        ["tasks", "Tasks"],
        ["sessions", "Sessions"],
      ]}
      label="Task explorer view"
    />
  );
}
