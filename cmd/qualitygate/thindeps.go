package main

import (
	"fmt"
	"io"
	"os/exec"
	"sort"
	"strings"
)

const modulePath = "github.com/graphit-labs/graphit-code/"

// storeAndParser are the packages that make the core heavy: the stores, the graph
// database and the parsers.
var storeAndParser = []string{
	modulePath + "internal/ast", modulePath + "internal/hub", modulePath + "internal/lancestore",
	modulePath + "internal/knowledge", "github.com/LadybugDB", "github.com/tree-sitter", "github.com/lancedb",
}

// lightSubpackages live under a heavy package's path but link none of it.
var lightSubpackages = map[string]bool{
	modulePath + "internal/hub/adapters/agent": true,
}

// thinBinaryLimits lists, for each lightweight runtime binary the launcher embeds, the
// packages it must never link. One stray import silently brings a store back into a
// process that starts on every hook or tool call.
var thinBinaryLimits = map[string][]string{
	"./cmd/hook":    append([]string{modulePath + "internal/memory", modulePath + "internal/task", modulePath + "internal/ai"}, storeAndParser...),
	"./cmd/githook": append([]string{modulePath + "internal/memory", modulePath + "internal/task", modulePath + "internal/ai"}, storeAndParser...),
	"./cmd/tray":    append([]string{modulePath + "internal/memory", modulePath + "internal/task", modulePath + "internal/ai"}, storeAndParser...),
	"./cmd/model":   storeAndParser,
	// The store binary links the table store itself, never the parsers or the Hub.
	"./cmd/store": {
		modulePath + "internal/ast", modulePath + "internal/hub", modulePath + "internal/knowledge",
		"github.com/LadybugDB", "github.com/tree-sitter",
	},
}

// checkThinBinaries fails when a thin binary links a package it must not.
func checkThinBinaries(limits map[string][]string, listDeps func(pkg string) ([]string, error), stdout io.Writer) error {
	fmt.Fprintln(stdout, "qualitygate: Lightweight binaries link no store")
	pkgs := make([]string, 0, len(limits))
	for pkg := range limits {
		pkgs = append(pkgs, pkg)
	}
	sort.Strings(pkgs)
	var violations []string
	for _, pkg := range pkgs {
		deps, err := listDeps(pkg)
		if err != nil {
			return fmt.Errorf("list dependencies of %s: %w", pkg, err)
		}
		for _, dep := range deps {
			if lightSubpackages[dep] {
				continue
			}
			for _, forbidden := range limits[pkg] {
				if dep == forbidden || strings.HasPrefix(dep, forbidden+"/") {
					violations = append(violations, pkg+" links "+dep)
					break
				}
			}
		}
	}
	if len(violations) > 0 {
		return fmt.Errorf("lightweight binaries link heavy packages:\n  %s", strings.Join(violations, "\n  "))
	}
	return nil
}

func goListDeps(pkg string) ([]string, error) {
	out, err := exec.Command("go", "list", "-deps", pkg).Output()
	if err != nil {
		return nil, err
	}
	return strings.Fields(string(out)), nil
}
