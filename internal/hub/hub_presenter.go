package hub

import (
	"context"
	"fmt"
	"sort"

	"github.com/graphit-labs/graphit-code/internal/output"
)

type HubPresenter struct {
	svc *HubService
	reg *RegistryManager
	p   *output.Printer
}

func NewHubPresenter(reg *RegistryManager) *HubPresenter {
	return &HubPresenter{
		svc: NewHubService(reg),
		reg: reg,
		p:   output.NewPrinter("hub"),
	}
}

func (c *HubPresenter) Install(ctx context.Context, entryID, alias, ide string, entryType ArtifactType) {
	c.p.Running("Installing %s...", entryID)

	result, err := c.svc.Install(ctx, entryID, alias, ide, entryType, "", "")
	if err != nil {
		c.p.Error("Install failed: %v", err)
		return
	}

	c.p.Success("Installed %s (%s) @%s", result.Name, result.ArtType, result.Version)
}

func (c *HubPresenter) Uninstall(ctx context.Context, entryID string, entryType ArtifactType, ide string) {
	c.p.Running("Removing %s...", entryID)

	if err := c.svc.Uninstall(ctx, entryID, entryType, true, ide, ""); err != nil {
		c.p.Error("Uninstall failed: %v", err)
		return
	}
	c.p.Success("Removed %s", entryID)
}

func (c *HubPresenter) Update(ctx context.Context, ide string) {
	c.p.Running("Checking for updates...")

	results := c.svc.UpdateAll(ctx, ide, "")
	if len(results) == 0 {
		c.p.Info("All artifacts are up to date.")
		return
	}

	updated := 0
	for artID, err := range results {
		if err != nil {
			c.p.StepWarn("Update failed for %q: %v", artID, err)
		} else {
			c.p.StepOK("Updated %s", artID)
			updated++
		}
	}

	if updated > 0 {
		c.p.Success("Updated %d artifact(s).", updated)
	} else {
		c.p.Info("All artifacts are up to date.")
	}
}

func (c *HubPresenter) UpdateOneArtifact(ctx context.Context, entryID string, entryType ArtifactType, ide string) {
	c.p.Running("Updating %s...", entryID)

	if err := c.svc.UpdateOne(ctx, entryID, entryType, ide, ""); err != nil {
		c.p.Error("Update failed: %v", err)
		return
	}
	c.p.Success("Updated %s", entryID)
}

func (c *HubPresenter) ListEntries(typeFilter ArtifactType) {
	entries := c.reg.ListEntries(typeFilter)
	if len(entries) == 0 {
		c.p.Info("No entries found.")
		return
	}

	sort.Slice(entries, func(i, j int) bool {
		if entries[i].Type != entries[j].Type {
			return entries[i].Type < entries[j].Type
		}
		return entries[i].ID < entries[j].ID
	})

	rows := make([][2]string, 0, len(entries))
	for _, e := range entries {
		name := e.Name
		if name == "" {
			name = e.ID
		}
		version := e.Latest
		if version == "" {
			version = "—"
		}
		rows = append(rows, [2]string{
			fmt.Sprintf("%-12s  %s", string(e.Type), name),
			fmt.Sprintf("%s  (%s)", e.ID, version),
		})
	}

	c.p.Table([2]string{"TYPE / NAME", "ID / VERSION"}, rows)
}

func (c *HubPresenter) SearchEntries(term string, typeFilter ArtifactType) {
	entries := c.reg.SearchEntries(term, typeFilter)
	if len(entries) == 0 {
		c.p.Info("No results for %q.", term)
		return
	}

	rows := make([][2]string, 0, len(entries))
	for _, e := range entries {
		name := e.Name
		if name == "" {
			name = e.ID
		}
		desc := e.Description
		if len(desc) > 60 {
			desc = desc[:57] + "..."
		}
		if desc == "" {
			desc = "—"
		}
		version := e.Latest
		if version == "" {
			version = "—"
		}
		rows = append(rows, [2]string{
			fmt.Sprintf("%-12s  %s", string(e.Type), name),
			fmt.Sprintf("%s  (%s)  %s", e.ID, version, desc),
		})
	}

	c.p.Table([2]string{"TYPE / NAME", "ID / VERSION / DESCRIPTION"}, rows)
}

func (c *HubPresenter) ShowEntry(entryID string, entryType ArtifactType) {
	entry := c.reg.GetEntry(entryID, entryType)
	if entry == nil {
		c.p.Error("Entry %q not found in registry.", entryID)
		return
	}

	c.p.Header("Artifact: %s", entry.Name)
	c.p.KeyValue("ID", entry.ID)
	c.p.KeyValue("Type", string(entry.Type))
	c.p.KeyValue("Latest", entry.Latest)
	if len(entry.Versions) > 0 {
		c.p.Step("Available versions:")
		for _, v := range entry.Versions {
			c.p.ListItem("%s", v)
		}
	}
	c.p.KeyValue("Description", entry.Description)
	if len(entry.Tags) > 0 {
		c.p.KeyValue("Tags", joinStrings(entry.Tags))
	}
	if len(entry.Dependencies) > 0 {
		c.p.Step("Dependencies:")
		for _, dep := range entry.Dependencies {
			c.p.ListItem("%s (%s)", dep.ID, dep.Type)
		}
	}
}

func (c *HubPresenter) ListProjects() {
	projects := c.reg.ListProjects()
	if len(projects) == 0 {
		c.p.Info("No projects registered.")
		return
	}

	rows := make([][2]string, 0, len(projects))
	for _, p := range projects {
		rows = append(rows, [2]string{p.Name, p.RemoteID})
	}

	c.p.Table([2]string{"PROJECT", "REMOTE ID"}, rows)
}

func (c *HubPresenter) Submit(ctx context.Context, entryID, localPath string, meta *Entry, version string) {
	c.p.Running("Publishing %s@%s to hub...", entryID, version)
	c.p.Step("Zipping artifact files")

	if err := c.reg.PublishEntry(ctx, entryID, localPath, meta, version); err != nil {
		c.p.Error("Publish failed: %v", err)
		return
	}
	c.p.Success("Published %s@%s", entryID, version)
}

func joinStrings(ss []string) string {
	result := ""
	for i, s := range ss {
		if i > 0 {
			result += ", "
		}
		result += s
	}
	return result
}

func (c *HubPresenter) Link(ctx context.Context, artifactID, sourcePath, ide string, artType ArtifactType) {
	c.p.Running("Linking %s from %s...", artifactID, sourcePath)

	result, err := c.svc.Link(ctx, artifactID, sourcePath, ide, artType, "")
	if err != nil {
		c.p.Error("Link failed: %v", err)
		return
	}

	for _, link := range result.Links {
		c.p.Step("%s", link)
	}
	c.p.Success("Linked %s (%s)", result.ArtifactID, result.ArtType)
}

func (c *HubPresenter) Unlink(ctx context.Context, artifactID string, artType ArtifactType, ide string) {
	c.p.Running("Unlinking %s...", artifactID)

	if err := c.svc.Unlink(ctx, artifactID, ide, artType, ""); err != nil {
		c.p.Error("Unlink failed: %v", err)
		return
	}
	c.p.Success("Unlinked %s", artifactID)
}
