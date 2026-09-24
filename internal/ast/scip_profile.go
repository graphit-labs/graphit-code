package ast

import (
	"crypto/sha256"
	"embed"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	scip "github.com/scip-code/scip/bindings/go/scip"
	"gopkg.in/yaml.v3"
)

//go:embed scip_profiles/*.yaml
var shippedSCIPProfiles embed.FS

// SCIPProfile describes semantic extraction from one indexer family. A project
// override lives beside its syntax queries, under ast.queries_dir/scip/.
type SCIPProfile struct {
	Parser          string               `yaml:"parser" json:"parser"`
	Family          string               `yaml:"family" json:"family"`
	Extensions      []string             `yaml:"extensions" json:"extensions"`
	Merge           bool                 `yaml:"merge,omitempty" json:"-"`
	Entities        *SCIPProfileEntities `yaml:"entities" json:"entities"`
	Relations       []string             `yaml:"relations" json:"relations"`
	Documentation   *bool                `yaml:"documentation" json:"documentation"`
	ExternalSymbols *bool                `yaml:"external_symbols" json:"external_symbols"`
}

type SCIPProfileEntities struct {
	Kinds  []string          `yaml:"kinds" json:"kinds"`
	Labels map[string]string `yaml:"labels,omitempty" json:"labels,omitempty"`
}

var scipGraphLabel = regexp.MustCompile(`^[A-Za-z][A-Za-z0-9_]*$`)

var scipRelationNames = map[string]bool{
	"CONTAINS": true, "REFERENCES": true, "IMPLEMENTS": true,
	"TYPE_DEFINITION": true, "DEFINITION": true,
}

var scipProfileFamilies = []string{
	"clang", "dart", "dotnet", "go", "java", "php", "python", "ruby", "rust", "typescript",
}

func readSCIPProfile(data []byte, path string) (SCIPProfile, error) {
	var profile SCIPProfile
	dec := yaml.NewDecoder(strings.NewReader(string(data)))
	dec.KnownFields(true)
	if err := dec.Decode(&profile); err != nil {
		return profile, fmt.Errorf("SCIP profile %s: %w", path, err)
	}
	var trailing any
	if err := dec.Decode(&trailing); err != io.EOF {
		if err == nil {
			err = fmt.Errorf("multiple YAML documents")
		}
		return profile, fmt.Errorf("SCIP profile %s: %w", path, err)
	}
	return profile, nil
}

func mergeSCIPProfile(base, over SCIPProfile) SCIPProfile {
	if !over.Merge {
		return over
	}
	out := base
	if over.Parser != "" {
		out.Parser = over.Parser
	}
	if over.Family != "" {
		out.Family = over.Family
	}
	if over.Extensions != nil {
		out.Extensions = over.Extensions
	}
	if over.Entities != nil {
		if out.Entities == nil {
			out.Entities = &SCIPProfileEntities{}
		}
		entities := *out.Entities
		if over.Entities.Kinds != nil {
			entities.Kinds = over.Entities.Kinds
		}
		if len(over.Entities.Labels) != 0 {
			entities.Labels = make(map[string]string, len(out.Entities.Labels)+len(over.Entities.Labels))
			for kind, label := range out.Entities.Labels {
				entities.Labels[kind] = label
			}
			for kind, label := range over.Entities.Labels {
				entities.Labels[kind] = label
			}
		}
		out.Entities = &entities
	}
	if over.Relations != nil {
		out.Relations = over.Relations
	}
	if over.Documentation != nil {
		out.Documentation = over.Documentation
	}
	if over.ExternalSymbols != nil {
		out.ExternalSymbols = over.ExternalSymbols
	}
	out.Merge = false
	return out
}

func validateSCIPProfile(profile SCIPProfile, family string) error {
	if profile.Parser != "scip" || profile.Family != family {
		return fmt.Errorf("SCIP profile %s must declare parser: scip and family: %s", family, family)
	}
	if len(profile.Extensions) == 0 || profile.Entities == nil || len(profile.Entities.Kinds) == 0 ||
		profile.Relations == nil || profile.Documentation == nil || profile.ExternalSymbols == nil {
		return fmt.Errorf("SCIP profile %s lacks extensions, entity kinds, relations, documentation or external_symbols", family)
	}
	for _, ext := range profile.Extensions {
		if scipFamilies[ext] != family {
			return fmt.Errorf("SCIP profile %s has unsupported extension %q", family, ext)
		}
	}
	for _, kind := range profile.Entities.Kinds {
		if kind != "*" {
			if _, ok := scip.SymbolInformation_Kind_value[kind]; !ok {
				return fmt.Errorf("SCIP profile %s has unknown entity kind %q", family, kind)
			}
		}
	}
	for kind, label := range profile.Entities.Labels {
		if _, ok := scip.SymbolInformation_Kind_value[kind]; !ok {
			return fmt.Errorf("SCIP profile %s has unknown label kind %q", family, kind)
		}
		if !scipGraphLabel.MatchString(label) {
			return fmt.Errorf("SCIP profile %s has invalid graph label %q", family, label)
		}
	}
	for _, relation := range profile.Relations {
		if !scipRelationNames[relation] {
			return fmt.Errorf("SCIP profile %s has unsupported relation %q", family, relation)
		}
	}
	return nil
}

func scipProfileFor(projectDir, family string) (SCIPProfile, error) {
	name := "scip-" + family + ".yaml"
	shippedPath := filepath.ToSlash(filepath.Join("scip_profiles", name))
	data, err := shippedSCIPProfiles.ReadFile(shippedPath)
	if err != nil {
		return SCIPProfile{}, fmt.Errorf("missing shipped SCIP profile %s: %w", family, err)
	}
	profile, err := readSCIPProfile(data, shippedPath)
	if err != nil {
		return SCIPProfile{}, err
	}
	for _, dir := range []string{runtimeQueriesDir(), userQueriesDir(), projectQueriesDir(projectDir)} {
		if dir == "" {
			continue
		}
		path := filepath.Join(dir, "scip", name)
		data, err := os.ReadFile(path)
		if os.IsNotExist(err) {
			continue
		}
		if err != nil {
			return SCIPProfile{}, fmt.Errorf("read SCIP profile %s: %w", path, err)
		}
		over, err := readSCIPProfile(data, path)
		if err != nil {
			return SCIPProfile{}, err
		}
		profile = mergeSCIPProfile(profile, over)
	}
	if err := validateSCIPProfile(profile, family); err != nil {
		return SCIPProfile{}, err
	}
	return profile, nil
}

func scipProfilesSignature(projectDir string) string {
	hash := sha256.New()
	for _, family := range scipProfileFamilies {
		profile, err := scipProfileFor(projectDir, family)
		if err != nil {
			_, _ = io.WriteString(hash, family+":invalid:"+err.Error()+"\n")
			continue
		}
		data, _ := json.Marshal(profile)
		_, _ = hash.Write(data)
	}
	return hex.EncodeToString(hash.Sum(nil))
}

func (profile SCIPProfile) supportsExt(ext string) bool {
	for _, supported := range profile.Extensions {
		if supported == ext {
			return true
		}
	}
	return false
}

func (profile SCIPProfile) entityLabel(kind scip.SymbolInformation_Kind) (string, bool) {
	name := kind.String()
	if _, known := scip.SymbolInformation_Kind_name[int32(kind)]; !known {
		name = "UnspecifiedKind"
	}
	selected := false
	for _, allowed := range profile.Entities.Kinds {
		if allowed == "*" || allowed == name {
			selected = true
			break
		}
	}
	if !selected {
		return "", false
	}
	if label := profile.Entities.Labels[name]; label != "" {
		return label, true
	}
	if name == "UnspecifiedKind" {
		return "Symbol", true
	}
	return name, true
}

func (profile SCIPProfile) allowsRelation(name string) bool {
	for _, relation := range profile.Relations {
		if relation == name {
			return true
		}
	}
	return false
}
