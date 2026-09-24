package ast

import (
	"fmt"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"

	scip "github.com/scip-code/scip/bindings/go/scip"
	"google.golang.org/protobuf/proto"
)

func scipUID(rel, symbol string) string {
	if scip.IsLocalSymbol(symbol) {
		return "scip-local:" + rel + ":" + symbol
	}
	return "scip:" + symbol
}

func scipDisplayName(info *scip.SymbolInformation, symbol string) string {
	if info != nil && info.GetDisplayName() != "" {
		return info.GetDisplayName()
	}
	// A missing display name must never affect identity. Retain the entire
	// symbol when no better name is available rather than inventing a match.
	return symbol
}

func scipDocumentation(info *scip.SymbolInformation) string {
	if info == nil {
		return ""
	}
	doc := strings.Join(info.GetDocumentation(), "\n")
	if signature := info.GetSignatureDocumentation().GetText(); signature != "" {
		if doc != "" {
			return signature + "\n\n" + doc
		}
		return signature
	}
	return doc
}

func scipProfileDocumentation(profile SCIPProfile, info *scip.SymbolInformation) string {
	if profile.Documentation == nil || !*profile.Documentation {
		return ""
	}
	return scipDocumentation(info)
}

func scipEnclosingSymbol(info *scip.SymbolInformation) string {
	if parent := info.GetEnclosingSymbol(); parent != "" {
		return parent
	}
	if info == nil || !scip.IsGlobalSymbol(info.GetSymbol()) {
		return ""
	}
	symbol, err := scip.ParseSymbol(info.GetSymbol())
	if err != nil || len(symbol.GetDescriptors()) < 2 {
		return ""
	}
	symbol.Descriptors = symbol.Descriptors[:len(symbol.Descriptors)-1]
	return scip.VerboseSymbolFormatter.FormatSymbol(symbol)
}

func scipMoreSpecificRange(candidate, current scip.Range) bool {
	if candidate.Start.Line != current.Start.Line {
		return candidate.Start.Line > current.Start.Line
	}
	if candidate.Start.Character != current.Start.Character {
		return candidate.Start.Character > current.Start.Character
	}
	if candidate.End.Line != current.End.Line {
		return candidate.End.Line < current.End.Line
	}
	return candidate.End.Character < current.End.Character
}

// decodeSCIP accepts only paths already selected by Graphit's ignore-aware
// discovery. A producer may index its entire workspace, including generated or
// ignored source; those documents must never enter the Graphit graph.
func decodeSCIP(data []byte, root string, allowed map[string]bool) (map[string]*parseCacheEntry, error) {
	profiles := make(map[string]SCIPProfile)
	for rel := range allowed {
		family := scipFamilies[strings.ToLower(filepath.Ext(rel))]
		if family == "" {
			continue
		}
		if _, ok := profiles[family]; !ok {
			profile, err := scipProfileFor(root, family)
			if err != nil {
				return nil, err
			}
			profiles[family] = profile
		}
	}
	return decodeSCIPWithProfiles(data, root, allowed, profiles)
}

func decodeSCIPWithProfiles(data []byte, root string, allowed map[string]bool, profiles map[string]SCIPProfile) (map[string]*parseCacheEntry, error) {
	var index scip.Index
	if err := proto.Unmarshal(data, &index); err != nil {
		return nil, fmt.Errorf("decode SCIP protobuf: %w", err)
	}
	canonicalRoot, err := filepath.EvalSymlinks(root)
	if err != nil {
		return nil, fmt.Errorf("resolve SCIP project root: %w", err)
	}
	entries := make(map[string]*parseCacheEntry)
	globalKinds := make(map[string]scip.SymbolInformation_Kind)
	for _, doc := range index.GetDocuments() {
		rel := doc.GetRelativePath()
		profile, ok := profiles[scipFamilies[strings.ToLower(filepath.Ext(rel))]]
		if !ok || !profile.supportsExt(strings.ToLower(filepath.Ext(rel))) || !allowed[rel] {
			continue
		}
		for _, info := range doc.GetSymbols() {
			if info != nil && info.GetSymbol() != "" && !scip.IsLocalSymbol(info.GetSymbol()) {
				if kind := info.GetKind(); kind != scip.SymbolInformation_UnspecifiedKind {
					globalKinds[info.GetSymbol()] = kind
				}
			}
		}
	}
	for _, doc := range index.GetDocuments() {
		rel := doc.GetRelativePath()
		profile, ok := profiles[scipFamilies[strings.ToLower(filepath.Ext(rel))]]
		if rel == "" || strings.HasPrefix(rel, "/") || path.Clean(rel) != rel || strings.Contains(rel, "\\") || !allowed[rel] ||
			!ok || !profile.supportsExt(strings.ToLower(filepath.Ext(rel))) {
			continue
		}
		abs := filepath.Join(root, filepath.FromSlash(rel))
		resolved, err := filepath.EvalSymlinks(abs)
		if err != nil {
			continue
		}
		inside, err := filepath.Rel(canonicalRoot, resolved)
		if err != nil || inside == ".." || strings.HasPrefix(inside, ".."+string(filepath.Separator)) {
			continue
		}
		if st, err := os.Stat(abs); err != nil || !st.Mode().IsRegular() {
			continue
		}
		entry := &parseCacheEntry{RelPath: rel, Language: strings.ToLower(doc.GetLanguage())}
		if entry.Language == "" {
			entry.Language = strings.TrimPrefix(strings.ToLower(filepath.Ext(rel)), ".")
		}
		for dir := path.Dir(rel); dir != "." && dir != ""; dir = path.Dir(dir) {
			entry.DirPaths = append(entry.DirPaths, dir)
		}
		infos := doc.SymbolTable()
		invalidDefinition := false
		for _, occ := range doc.GetOccurrences() {
			if occ.GetSymbol() == "" || occ.GetSymbolRoles()&int32(scip.SymbolRole_Definition) == 0 {
				continue
			}
			rng, ok := occ.SourceRange()
			if !ok || rng.Validate() != nil {
				invalidDefinition = true
				break
			}
		}
		if invalidDefinition {
			continue
		}
		definitions := make(map[string]scip.Range)
		seen := make(map[string]bool)
		for _, occ := range doc.GetOccurrences() {
			symbol := occ.GetSymbol()
			if symbol == "" || occ.GetSymbolRoles()&int32(scip.SymbolRole_Definition) == 0 {
				continue
			}
			rng, ok := occ.SourceRange()
			if !ok || rng.Validate() != nil {
				continue
			}
			uid := scipUID(rel, symbol)
			if seen[uid] {
				continue
			}
			info := infos[symbol]
			kind := info.GetKind()
			if kind == scip.SymbolInformation_UnspecifiedKind && !scip.IsLocalSymbol(symbol) {
				kind = globalKinds[symbol]
			}
			label, selected := profile.entityLabel(kind)
			if !selected {
				continue
			}
			seen[uid] = true
			if enclosing, ok := occ.EnclosingSourceRange(); ok && enclosing.Validate() == nil {
				definitions[uid] = enclosing
			} else {
				definitions[uid] = rng
			}
			ent := cachedEntity{UID: uid, Label: label, Name: scipDisplayName(info, symbol), Path: rel,
				Line: int(rng.Start.Line) + 1, EndLine: int(rng.End.Line) + 1,
				Lang: entry.Language, IsExported: !scip.IsLocalSymbol(symbol)}
			if *profile.Documentation {
				ent.Docstring = scipDocumentation(info)
			}
			entry.Entities = append(entry.Entities, ent)
		}
		for _, info := range doc.GetSymbols() {
			if info == nil || info.GetSymbol() == "" {
				continue
			}
			kind := info.GetKind()
			if kind == scip.SymbolInformation_UnspecifiedKind && !scip.IsLocalSymbol(info.GetSymbol()) {
				kind = globalKinds[info.GetSymbol()]
			}
			label, selected := profile.entityLabel(kind)
			if !selected {
				continue
			}
			source := scipUID(rel, info.GetSymbol())
			if !seen[source] {
				seen[source] = true
				entry.Entities = append(entry.Entities, cachedEntity{UID: source,
					Label: label, Name: scipDisplayName(info, info.GetSymbol()),
					Path: rel, Lang: entry.Language, Docstring: scipProfileDocumentation(profile, info),
					IsExported: !scip.IsLocalSymbol(info.GetSymbol()), IsStub: true})
			}
			if parent := scipEnclosingSymbol(info); parent != "" && profile.allowsRelation("CONTAINS") {
				parentKind := infos[parent].GetKind()
				if parentKind == scip.SymbolInformation_UnspecifiedKind && !scip.IsLocalSymbol(parent) {
					parentKind = globalKinds[parent]
				}
				if parentLabel, allowed := profile.entityLabel(parentKind); allowed {
					entry.ContainsEdges = append(entry.ContainsEdges, cachedContainsEdge{ParentUID: scipUID(rel, parent), ChildUID: source,
						ParentLabel: parentLabel, ChildLabel: label})
				}
			}
			for _, relation := range info.GetRelationships() {
				if relation == nil || relation.GetSymbol() == "" {
					continue
				}
				target := scipUID(rel, relation.GetSymbol())
				if relation.GetIsImplementation() && profile.allowsRelation("IMPLEMENTS") {
					entry.Inheritance = append(entry.Inheritance, cachedInheritance{ChildUID: source,
						ParentUID: target, RelType: "IMPLEMENTS", Path: rel})
				}
				// SCIP relationship flags are independent. In particular, an
				// implementation may also be a reference to the same symbol.
				for _, flag := range []struct {
					enabled bool
					kind    string
				}{
					{relation.GetIsReference(), "REFERENCES"},
					{relation.GetIsTypeDefinition(), "TYPE_DEFINITION"},
					{relation.GetIsDefinition(), "DEFINITION"},
				} {
					if flag.enabled && profile.allowsRelation(flag.kind) {
						entry.References = append(entry.References, cachedReference{SourceUID: source, TargetUID: target,
							RelType: flag.kind, Path: rel, Lang: entry.Language})
					}
				}
			}
		}
		if profile.allowsRelation("REFERENCES") {
			for _, occ := range doc.GetOccurrences() {
				symbol := occ.GetSymbol()
				if symbol == "" || occ.GetSymbolRoles()&int32(scip.SymbolRole_Definition) != 0 {
					continue
				}
				rng, ok := occ.SourceRange()
				if !ok || rng.Validate() != nil {
					continue
				}
				source := rel
				var bestRange scip.Range
				found := false
				for uid, defRange := range definitions {
					if uid == scipUID(rel, symbol) || !defRange.Contains(rng.Start) {
						continue
					}
					if !found || scipMoreSpecificRange(defRange, bestRange) || (defRange == bestRange && uid < source) {
						bestRange, source, found = defRange, uid, true
					}
				}
				entry.References = append(entry.References, cachedReference{SourceUID: source,
					TargetUID: scipUID(rel, symbol), RelType: "REFERENCES", Line: int(rng.Start.Line) + 1,
					Path: rel, Lang: entry.Language})
			}
		}
		entries[abs] = entry
	}
	// ExternalSymbols supplies hover metadata for dependency targets that have no
	// definition document in this index. Preserve it on a canonical stub.
	externalInfos := make(map[string]*scip.SymbolInformation)
	for _, info := range index.GetExternalSymbols() {
		if info != nil && info.GetSymbol() != "" && !scip.IsLocalSymbol(info.GetSymbol()) {
			externalInfos[scipUID("", info.GetSymbol())] = info
		}
	}
	defined := make(map[string]bool)
	paths := make([]string, 0, len(entries))
	for abs, entry := range entries {
		paths = append(paths, abs)
		for _, ent := range entry.Entities {
			if !ent.IsStub {
				defined[ent.UID] = true
			}
		}
	}
	sort.Strings(paths)
	for _, abs := range paths {
		entry := entries[abs]
		kept := entry.Entities[:0]
		for _, ent := range entry.Entities {
			if !ent.IsStub || !defined[ent.UID] {
				kept = append(kept, ent)
			}
		}
		entry.Entities = kept
	}
	for _, abs := range paths {
		entry := entries[abs]
		for _, ent := range entry.Entities {
			defined[ent.UID] = true
		}
	}
	for _, abs := range paths {
		entry := entries[abs]
		profile := profiles[scipFamilies[strings.ToLower(filepath.Ext(abs))]]
		appendExternal := func(targetUID string) {
			info := externalInfos[targetUID]
			if info == nil || defined[targetUID] || !*profile.ExternalSymbols {
				return
			}
			label, allowed := profile.entityLabel(info.GetKind())
			if !allowed {
				label = "Symbol"
			}
			entry.Entities = append(entry.Entities, cachedEntity{UID: targetUID,
				Label: label, Name: scipDisplayName(info, info.GetSymbol()),
				Docstring: scipProfileDocumentation(profile, info), Lang: entry.Language, IsDep: true, IsStub: true})
			defined[targetUID] = true
		}
		for _, ref := range entry.References {
			appendExternal(ref.TargetUID)
		}
		for _, inh := range entry.Inheritance {
			appendExternal(inh.ParentUID)
		}
	}
	return entries, nil
}
