import { WorkspaceNow } from "./WorkspaceNow";
import { useState } from "react";
import { Link } from "react-router-dom";
import { ArrowUpRight } from "lucide-react";
import { useAppStore } from "@/store/appStore";
import { useProviderMode } from '@/store/providerMode';
import { useTaskWorkspace } from "@/components/task/taskWorkspaceState";
import { agentFeaturesEnabled } from "@/lib/utils";
import {
  WorkPage,
  WorkHeader,
  WorkSection,
  WorkTabs,
  FactList,
} from "@/components/shared/EngineeringUI";

export default function WorkspacePage() {
  const { projects, webMode } = useAppStore();
  const provider = useProviderMode();
  const { selection } = useTaskWorkspace();
  const project = projects.find(item => item.id === selection?.project_id && item.dir === selection?.project_dir);
  const [intent, setIntent] = useState("now");
  const activeIntent = intent === 'share' && provider !== 'broker' ? 'now' : intent;
  const jobs: Record<string, Array<[string, string, string]>> = {
    continue: [
      [
        "Resume a request",
        "Recover intent, decisions and the next action.",
        "/task/sessions",
      ],
      [
        "Review a delivery",
        "Inspect requirements, ownership, checks and evidence.",
        "/task/explorer",
      ],
    ],
    understand: [
      [
        "Investigate code",
        "Find implementation and trace indexed relationships.",
        "/ast/explorer",
      ],
      [
        "Read system contracts",
        "Find maintained knowledge and inspect its sources.",
        "/knowledge/explorer",
      ],
      [
        "Recall decisions",
        "Read project memory and the reasons behind conventions.",
        "/memory/explorer/project",
      ],
      ...(agentFeaturesEnabled()
        ? [
            [
              "Investigate shared context",
              "Run an agent over a selected set of artifacts.",
              "/live",
            ] as [string, string, string],
          ]
        : []),
    ],
    share: [
      [
        "Discover reusable context",
        "Inspect versions and publishers before installing.",
        "/hub/registry",
      ],
      ...(!webMode
        ? [
            [
              "Maintain project artifacts",
              "Review owned, installed and linked context.",
              "/hub/local",
            ] as [string, string, string],
          ]
        : []),
      [
        "Publish an artifact",
        "Package, describe and review a reusable contribution.",
        "/hub/upload",
      ],
    ],
    operate: [
      [
        "Manage the ecosystem",
        "Choose projects and maintain cluster relationships.",
        "/system/ecosystem",
      ],
      [
        "Inspect the daemon",
        "Diagnose background work and connect an agent.",
        "/system/daemon",
      ],
      [
        "Review Dream work",
        "Read the results of autonomous maintenance.",
        "/system/dream",
      ],
    ],
  };
  return (
    <WorkPage>
      <WorkHeader
        title="Engineering workspace"
        description="Choose the work you need to do. Project context, ownership and evidence carry across the tools."
      />
      <WorkTabs value={activeIntent} onChange={setIntent} items={[
        ["now", "Now"], ["continue", "Continue work"], ["understand", "Understand"], ["share", "Reuse & share"], ["operate", "Operate"],
      ].filter(([key]) => key !== 'share' || provider === 'broker') as [string, string][]} label="Engineering intent" />
      {activeIntent === "now" ? <WorkspaceNow /> : <div className="workspace-launchpad">
        <section>
          <WorkSection title="What do you need to do?">
            <div className="workspace-jobs">
              {jobs[activeIntent].map(([title, description, path]) => (
                <Link to={path} key={path}>
                  <span>
                    <strong>{title}</strong>
                    <small>{description}</small>
                  </span>
                  <ArrowUpRight size={18} />
                </Link>
              ))}
            </div>
          </WorkSection>
        </section>
        <aside>
          <WorkSection title="Project context">
            {project?.description && <div className="markdown-preview"><MarkdownContent content={project.description} /></div>}
            <FactList
              items={[
                ["Project ID", selection?.project_id],
                ["Data version", selection?.project_version],
                ["Directory", selection?.project_dir],
                ...(project?.registered_at ? [["Registered", new Date(project.registered_at).toLocaleDateString()] as const] : []).map(([label, value]): [string, string] => [label, value]),
                ...Object.entries(project?.cluster ?? {}).map(([key, values]): [string, string] => [key, values.join(", ")]),
              ]}
            />
          </WorkSection>
          <Link className="work-button" to="/system/ecosystem">
            Manage projects
          </Link>
        </aside>
      </div>}
    </WorkPage>
  );
}
import { MarkdownContent } from "@/components/wiki/WikiMarkdown";
