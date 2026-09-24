package ast

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	scip "github.com/scip-code/scip/bindings/go/scip"
)

func TestEverySCIPFamilyHasDeclarativeProfile(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	root := t.TempDir()
	syntax, err := loadQueriesFromDir("queries")
	if err != nil {
		t.Fatal(err)
	}
	for _, profile := range syntax {
		if profile.Parser == "scip" {
			t.Fatalf("SCIP profile %q leaked into syntax grammar loader", profile.Language)
		}
	}
	for _, family := range scipProfileFamilies {
		profile, err := scipProfileFor(root, family)
		if err != nil {
			t.Fatalf("%s: %v", family, err)
		}
		if profile.Parser != "scip" || profile.Family != family || !*profile.Documentation || !*profile.ExternalSymbols {
			t.Fatalf("incomplete profile %s: %+v", family, profile)
		}
		for ext, expected := range scipFamilies {
			if expected == family && !profile.supportsExt(ext) {
				t.Errorf("%s profile omits %s", family, ext)
			}
		}
		for _, relation := range []string{"CONTAINS", "REFERENCES", "IMPLEMENTS", "TYPE_DEFINITION", "DEFINITION"} {
			if !profile.allowsRelation(relation) {
				t.Errorf("%s profile omits %s", family, relation)
			}
		}
		for kind := range scip.SymbolInformation_Kind_name {
			if label, ok := profile.entityLabel(scip.SymbolInformation_Kind(kind)); !ok || label == "" {
				t.Errorf("%s profile omits SCIP kind %d", family, kind)
			}
		}
	}
}

func TestSCIPProfileProjectOverridesGlobalAndChangesSignature(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	root := t.TempDir()
	global := filepath.Join(userQueriesDir(), "scip", "scip-go.yaml")
	project := filepath.Join(projectQueriesDir(root), "scip", "scip-go.yaml")
	for _, path := range []string{global, project} {
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	before := scipProfilesSignature(root)
	if err := os.WriteFile(global, []byte("merge: true\nentities:\n  labels:\n    Function: GlobalFunction\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	globalSignature := scipProfilesSignature(root)
	if before == globalSignature {
		t.Fatal("global profile edit did not change SCIP signature")
	}
	profile, err := scipProfileFor(root, "go")
	if err != nil {
		t.Fatal(err)
	}
	if label, _ := profile.entityLabel(scip.SymbolInformation_Function); label != "GlobalFunction" {
		t.Fatalf("global override ignored: %s", label)
	}
	if err := os.WriteFile(project, []byte("merge: true\nentities:\n  labels:\n    Function: ProjectFunction\nrelations: [REFERENCES]\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	profile, err = scipProfileFor(root, "go")
	if err != nil {
		t.Fatal(err)
	}
	if label, _ := profile.entityLabel(scip.SymbolInformation_Function); label != "ProjectFunction" ||
		profile.allowsRelation("IMPLEMENTS") || !profile.allowsRelation("REFERENCES") {
		t.Fatalf("project override did not win: %+v", profile)
	}
	if scipProfilesSignature(root) == globalSignature {
		t.Fatal("project profile edit did not change SCIP signature")
	}
}

func TestSCIPProfileRejectsInvalidOverride(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	root := t.TempDir()
	path := filepath.Join(projectQueriesDir(root), "scip", "scip-go.yaml")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	for _, yaml := range []string{
		"merge: true\nentities:\n  kinds: [DoesNotExist]\n",
		"merge: true\nrelations: [CALLS]\n",
		"merge: true\nentities:\n  labels:\n    Function: 'Bad-Label'\n",
		"merge: true\nunknown_field: true\n",
	} {
		if err := os.WriteFile(path, []byte(yaml), 0o644); err != nil {
			t.Fatal(err)
		}
		if _, err := scipProfileFor(root, "go"); err == nil || !strings.Contains(err.Error(), "SCIP profile") {
			t.Fatalf("invalid profile %q accepted: %v", yaml, err)
		}
	}
}

func TestSCIPProfileEditDoesNotReindexWhenSCIPDisabled(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "false")
	root := t.TempDir()
	before := scipConfigurationSignature(root)
	path := filepath.Join(projectQueriesDir(root), "scip", "scip-go.yaml")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("merge: true\nentities:\n  labels:\n    Function: Routine\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if got := scipConfigurationSignature(root); got != before {
		t.Fatal("disabled SCIP profile edit changed AST configuration signature")
	}
}
