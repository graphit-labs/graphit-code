import type { ReactNode } from "react";
import { WorkBadge, WorkEmpty, WorkHeader, WorkPage } from "@/components/shared/EngineeringUI";
import { useAppStore } from "@/store/appStore";

export function LocalProjectRoute({ children, capability }: { children: ReactNode; capability: string }) {
  const { activeProjectOrigin, projectName, activeProjectId } = useAppStore();
  if (activeProjectOrigin !== "hub") return <>{children}</>;

  return (
    <WorkPage>
      <WorkHeader
        title={capability}
        description="This operation requires a local checkout and cannot run against a published Hub artifact."
        actions={<WorkBadge tone="info">Hub · remote</WorkBadge>}
      />
      <WorkEmpty title="Local workspace required">
        <span>
          <strong>{projectName || activeProjectId}</strong> remains the selected project for supported
          remote modules. Choose its Workspace copy — or another local project — in the global Project
          selector to use {capability.toLowerCase()}.
        </span>{" "}
        <code>{activeProjectId}</code>
      </WorkEmpty>
    </WorkPage>
  );
}
