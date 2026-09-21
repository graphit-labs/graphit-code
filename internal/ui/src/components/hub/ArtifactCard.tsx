import { StyledSelect } from "@/components/shared/StyledSelect";
import { useState } from "react";
import type { RegistryEntry, InstalledArtifact } from "@/api/hub";
import {
  WorkSection,
  WorkNotice,
  FactList,
} from "@/components/shared/EngineeringUI";
export interface ArtifactCardProps {
  variant: "registry" | "project" | "imported";
  entry?: RegistryEntry | null;
  installedInfo?: InstalledArtifact | null;
  webMode?: boolean;
  activeProjectId?: string;
  projectPath?: string;
  onInstall?: (
    id: string,
    type: string,
    withAlias?: boolean,
    version?: string,
  ) => void;
  onUninstall?: (entry: RegistryEntry, installed: InstalledArtifact) => void;
  onUpdate?: (id: string, type: string) => void;
  onRemove?: (art: InstalledArtifact) => void;
  onUnpublish?: (id: string, type: string) => void;
  onSubmit?: (art: InstalledArtifact) => void;
  onUnlink?: (art: InstalledArtifact) => void;
  clusterLabels?: Record<string, string>;
}

export function ArtifactCard({
  variant,
  entry,
  installedInfo: art,
  webMode = false,
  activeProjectId = "",
  onInstall,
  onUninstall,
  onUpdate,
  onRemove,
  onUnpublish,
  onSubmit,
  onUnlink,
  clusterLabels,
}: ArtifactCardProps) {
  const [version, setVersion] = useState("");
  const isRegistry = variant === "registry";
  const name = isRegistry ? entry?.name : art?.registry_name || art?.local_id;
  const type = isRegistry ? entry?.type : art?.type;
  const managed =
    isRegistry &&
    (art?.origin === "managed" ||
      art?.origin === "publish" ||
      (!!entry?.project_id && entry.project_id === activeProjectId));
  return (
    <article className="artifact-inspector">
      <header>
        <span className="status-pill">{type}</span>
        <h2>{name}</h2>
        <p>
          {isRegistry
            ? entry?.description
            : art?.registry_description || "No description available."}
        </p>
      </header>
      <WorkSection title="Identity & origin">
        <FactList
          items={[
            ["Identifier", isRegistry ? entry?.id : art?.local_id],
            [
              "Publisher",
              isRegistry ? entry?.author?.username : art?.registry_author,
            ],
            ["Project", entry?.project_id],
            ["Installed version", art?.version],
            ["Registry version", entry?.latest || art?.registry_version],
            ["Alias", art?.alias],
            ["Local path", art?.path],
            ["Origin", art?.origin],
          ]}
        />
      </WorkSection>
      <div className="work-actions mb-6">
        {(isRegistry ? entry?.tags : art?.registry_tags)?.map((t) => (
          <span className="status-pill" key={t}>
            {t}
          </span>
        ))}
      </div>
      {clusterLabels &&
        ["knowledge", "ast", "language", "framework"].includes(type || "") &&
        Object.keys(clusterLabels).length > 0 && (
          <WorkSection title="Cluster context">
            <FactList items={Object.entries(clusterLabels)} />
          </WorkSection>
        )}
      {!!art?.registry_dependencies?.length && (
        <WorkSection title="Dependencies">
          <FactList
            items={art.registry_dependencies.map((d) => [
              d.type + "/" + d.id,
              d.version,
            ])}
          />
        </WorkSection>
      )}
      <WorkSection title={isRegistry ? "Use this artifact" : "Manage artifact"}>
        {isRegistry &&
          entry &&
          (webMode ? (
            <a
              className="work-button primary"
              href={
                "/api/download/" +
                entry.id +
                "?type=" +
                encodeURIComponent(entry.type)
              }
            >
              Download
            </a>
          ) : managed ? (
            <WorkNotice title="Managed by this project">
              Publish updates from Project artifacts.
            </WorkNotice>
          ) : art ? (
            <button
              className="work-button danger"
              onClick={() => onUninstall?.(entry, art)}
            >
              Remove installation
            </button>
          ) : (
            <div className="work-form">
              <label className="work-field">
                <span>Version to install</span>
                <StyledSelect
                  value={version}
                  onChange={(e) => setVersion(e.target.value)}
                >
                  <option value="">Latest ({entry.latest})</option>
                  {entry.versions?.map((v) => (
                    <option key={v}>{v}</option>
                  ))}
                </StyledSelect>
              </label>
              <div className="work-actions">
                <button
                  className="work-button primary"
                  onClick={() =>
                    onInstall?.(
                      entry.id,
                      entry.type,
                      false,
                      version || undefined,
                    )
                  }
                >
                  Install
                </button>
                <button
                  className="work-button"
                  onClick={() =>
                    onInstall?.(
                      entry.id,
                      entry.type,
                      true,
                      version || undefined,
                    )
                  }
                >
                  Install with alias
                </button>
              </div>
            </div>
          ))}
        {variant === "project" && art && (
          <>
            <p className="text-xs text-muted-foreground mb-4">
              {art.published
                ? "Published in the registry."
                : "Local draft, not yet published."}
            </p>
            <div className="work-actions">
              {onSubmit && (
                <button
                  className="work-button primary"
                  onClick={() => onSubmit(art)}
                >
                  {art.published ? "Publish revision" : "Publish artifact"}
                </button>
              )}
              {art.published && onUnpublish && (
                <button
                  className="work-button danger"
                  onClick={() => onUnpublish(art.local_id, art.type)}
                >
                  Unpublish
                </button>
              )}
            </div>
          </>
        )}
        {variant === "imported" && art && (
          <div className="work-actions">
            {art.origin === "link" && onUnlink ? (
              <button
                className="work-button danger"
                onClick={() => onUnlink(art)}
              >
                Unlink
              </button>
            ) : (
              <>
                {art.has_update && onUpdate && (
                  <button
                    className="work-button primary"
                    onClick={() =>
                      onUpdate(art.remote_id || art.local_id, art.type)
                    }
                  >
                    Update to {art.registry_version || "latest"}
                  </button>
                )}
                {onRemove && (
                  <button
                    className="work-button danger"
                    onClick={() => onRemove(art)}
                  >
                    Remove
                  </button>
                )}
              </>
            )}
          </div>
        )}
      </WorkSection>
    </article>
  );
}
