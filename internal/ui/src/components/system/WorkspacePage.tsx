import { WorkspaceNow } from "./WorkspaceNow";
import { useState } from "react";
import { Link } from "react-router-dom";
import { ArrowUpRight } from "lucide-react";
import { useAppStore } from "@/store/appStore";
import { agentFeaturesEnabled } from "@/lib/utils";
import {
  WorkPage,
  WorkHeader,
  WorkSection,
  WorkTabs,
  FactList,
} from "@/components/shared/EngineeringUI";

export default function WorkspacePage() {
  const {
    projects,
    activeProjectDir,
    webMode,
  } = useAppStore();
  const [intent, setIntent] = useState("now");
  const active = projects.find((p) => p.dir === activeProjectDir);
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
      <WorkTabs value={intent} onChange={setIntent} items={[
        ["now", "Now"], ["continue", "Continue work"], ["understand", "Understand"], ["share", "Reuse & share"], ["operate", "Operate"],
      ]} label="Engineering intent" />
      {intent === "now" ? <WorkspaceNow /> : <div className="workspace-launchpad">
        <section>
          <WorkSection title="What do you need to do?">
            <div className="workspace-jobs">
              {jobs[intent].map(([title, description, path]) => (
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
            <FactList
              items={[
                ["Description", active?.description],
                ["Directory", active?.dir],
                ["Project ID", active?.id],
                [
                  "Registered",
                  active?.registered_at
                    ? new Date(active.registered_at).toLocaleDateString()
                    : "—",
                ],
                ...Object.entries(active?.cluster || {}).map(
                  ([k, vs]): [string, string] => [k, vs.join(", ")],
                ),
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
