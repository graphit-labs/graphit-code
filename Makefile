.PHONY: build build-all install clean fmt vet run ui ui-dev setup-lbug \
       fetch-ort-linux fetch-ort-darwin fetch-ort-windows fetch-model lint \
       ui-lint ci check test build-windows-native \
       grammars grammars-treesitter grammars-antlr grammars-clean

MODULE   := github.com/graphit-labs/graphit-code
CMD      := ./cmd/graphit
BIN_DIR  := .build

BRAND        ?= graphit
DISPLAY_NAME ?= Graphit Code: A Powerful Agent Harness for Enterprise Software Ecosystems
VERSION      ?= dev
GITHUB_REPO  ?= graphit-labs/graphit-code
DEFAULT_HUB_REPO    ?=
DEFAULT_MEMORY_REPO ?=
SELF_UPDATE_URL ?=
COMPILE_CONFIG ?=

LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.Brand=$(BRAND)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.DisplayName=$(DISPLAY_NAME)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/version.Version=$(VERSION)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.GitHubRepo=$(GITHUB_REPO)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.DefaultHubRepoURL=$(DEFAULT_HUB_REPO)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.DefaultMemoryRepoURL=$(DEFAULT_MEMORY_REPO)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.SelfUpdateURL=$(SELF_UPDATE_URL)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/config.CompiledDefaults=$(COMPILE_CONFIG)'

BUILD_TAGS := fts5

ORT_VERSION  := 1.25.0
ORT_CACHE    := /tmp/onnxruntime-cache

MODEL_REPO   := mrsladoje/CodeRankEmbed-onnx-int8
MODEL_CACHE  := /tmp/coderankembed-cache

LBUG_VERSION := v0.13.1
LBUG_MOD     := $(shell go env GOPATH)/pkg/mod/github.com/!ladybug!d!b/go-ladybug@$(LBUG_VERSION)
LBUG_CACHE   := /tmp/lbug-cache

LBUG_PLATFORMS ?= $(shell uname -s | sed 's/Darwin/darwin/;s/Linux/linux-amd64/;s/MINGW.*/windows/')

# ═══════════════════════════════════════════════════════════════════════════════
# Grammar Configuration
# ═══════════════════════════════════════════════════════════════════════════════
#
# To add a new tree-sitter grammar:
#   1. Add to the appropriate category below (SMACKER, EXTERNAL, or LOCAL)
#   2. If EXTERNAL, add the Go module version to GRAMMAR_EXTERNAL_<LANG>_MOD
#   3. If it needs alloc.c, add to TS_GRAMMARS_ALLOC
#   4. If the source dir is non-standard, add to TS_GRAMMAR_SRCDIR_<LANG>
#   5. Run: make grammars-treesitter
#
# To add a new ANTLR grammar:
#   1. Add the grammar name to ANTLR_GRAMMARS
#   2. Implement the grammar under internal/ast/antlr/<grammar>/
#   3. Create the build-tag driver in cmd/graphit-antlr-sidecar/
#   4. Run: make grammars-antlr

# ── Tree-sitter ───────────────────────────────────────────────────────────────

# Output directories.
TS_OUTDIR     := .build/grammars/treesitter
ANTLR_OUTDIR  := .build/grammars/antlr

# Go module cache.
GOMODCACHE    := $(shell go env GOMODCACHE)
SMACKER_DIR    = $(shell find $(GOMODCACHE)/github.com/smacker/go-tree-sitter* -maxdepth 0 2>/dev/null | head -1)

# Platform-specific shared library extension.
ifeq ($(OS),Windows_NT)
  SHLIB_EXT := .dll
else ifeq ($(shell uname -s),Darwin)
  SHLIB_EXT := .dylib
else
  SHLIB_EXT := .so
endif

# Compiler flags.
TS_CC         := $(CC)
TS_CXX        := $(CXX)
TS_CFLAGS     := -shared -fPIC -O2 -std=c11
TS_CXXFLAGS   := -shared -fPIC -O2 -std=c++14

# Category A: smacker/go-tree-sitter grammars (source under SMACKER_DIR/<lang>/).
# Simple: no scanner or scanner.c that doesn't need alloc.c.
TS_GRAMMARS_SMACKER_SIMPLE := c golang java protobuf \
    dockerfile elixir groovy hcl javascript lua python swift toml \
    kotlin php sql

# Smacker grammars that need alloc.c linked in.
TS_GRAMMARS_SMACKER_ALLOC := bash cpp csharp html ruby rust scala

# Smacker grammars with non-standard source subdirectories.
TS_GRAMMAR_SRCDIR_markdown   := markdown/tree-sitter-markdown
TS_GRAMMAR_SRCDIR_tsx        := typescript/tsx
TS_GRAMMAR_SRCDIR_typescript := typescript/typescript
TS_GRAMMARS_SMACKER_SUBDIR := markdown tsx typescript

# Smacker grammars with C++ scanner (scanner.cc).
TS_GRAMMARS_SMACKER_CXX := yaml

# Category B: External Go module grammars (separate packages).
# Format: <lang>:<go-module-path>@<version>[/<subdir>]
TS_GRAMMARS_EXTERNAL := \
    json:github.com/tree-sitter/tree-sitter-json@v0.24.8 \
    xml:github.com/tree-sitter-grammars/tree-sitter-xml@v0.7.0/xml \
    zig:github.com/tree-sitter-grammars/tree-sitter-zig@v1.1.2 \
    haskell:github.com/tree-sitter/tree-sitter-haskell@v0.23.1 \
    julia:github.com/tree-sitter/tree-sitter-julia@v0.25.0 \
    dart:github.com/!user!nobody14/tree-sitter-dart@v0.0.0-20260508020638-507c5546dc73

# Category C: Local vendored grammars (under internal/ast/treesitter/<lang>/).
TS_GRAMMARS_LOCAL := clojure graphql objc r

# All tree-sitter grammars (computed).
TS_ALL_SMACKER := $(TS_GRAMMARS_SMACKER_SIMPLE) $(TS_GRAMMARS_SMACKER_ALLOC) \
    $(TS_GRAMMARS_SMACKER_SUBDIR) $(TS_GRAMMARS_SMACKER_CXX)
TS_ALL_EXTERNAL := $(foreach spec,$(TS_GRAMMARS_EXTERNAL),$(firstword $(subst :, ,$(spec))))
TS_ALL := $(TS_ALL_SMACKER) $(TS_ALL_EXTERNAL) $(TS_GRAMMARS_LOCAL)

# ── ANTLR ─────────────────────────────────────────────────────────────────────

# All ANTLR grammars. Each must have a build tag `grammar_<name>`.
ANTLR_GRAMMARS := plsql postgresql tsql db2 cobol85

# ── Default grammars to embed in the launcher ─────────────────────────────────
# All tree-sitter grammars (use post-rename names: go, c-sharp, proto).
DEFAULT_TS_GRAMMARS := go python javascript typescript tsx java kotlin \
    rust c-sharp cpp c ruby php swift dart sql markdown yaml \
    json html xml proto dockerfile elixir groovy hcl lua toml \
    bash scala zig haskell julia clojure graphql objc r

# All ANTLR sidecar binaries.
DEFAULT_ANTLR_GRAMMARS := plsql postgresql tsql db2 cobol85

# ── Conditional grammar dependencies ──────────────────────────────────────────
# Pass SKIP_GRAMMARS=1 and/or SKIP_ANTLR_GRAMMARS=1 to skip compilation+bundling.
ifndef SKIP_GRAMMARS
  GRAMMAR_DEPS := grammars-treesitter
else
  GRAMMAR_DEPS :=
endif

ifndef SKIP_ANTLR_GRAMMARS
  ANTLR_DEPS := grammars-antlr
else
  ANTLR_DEPS :=
endif

# ═══════════════════════════════════════════════════════════════════════════════
# Grammar Build Targets
# ═══════════════════════════════════════════════════════════════════════════════

# compile_ts_grammar <name> <src_dir> <include_dirs> [ALLOC=1] [CXX=1]
# Compiles a tree-sitter grammar from C/C++ source into a shared library.
define compile_ts_grammar
	name="$(1)"; \
	src="$(2)"; \
	inc="$(3)"; \
	alloc="$(4)"; \
	cxx="$(5)"; \
	output="$(TS_OUTDIR)/tree-sitter-$${name}$(SHLIB_EXT)"; \
	parser_c=""; \
	if [ -f "$${src}/parser.c" ]; then parser_c="$${src}/parser.c"; \
	elif [ -f "$${src}/parser.c.inc" ]; then parser_c="$${src}/parser.c.inc"; \
	else echo "  ✗ $${name}: parser.c not found in $${src}"; exit 1; fi; \
	scanner_c=""; scanner_cc=""; \
	if [ -f "$${src}/scanner.c" ]; then scanner_c="$${src}/scanner.c"; \
	elif [ -f "$${src}/scanner.c.inc" ]; then scanner_c="$${src}/scanner.c.inc"; fi; \
	if [ -f "$${src}/scanner.cc" ]; then scanner_cc="$${src}/scanner.cc"; fi; \
	iflags=""; \
	for d in $${inc}; do iflags="$${iflags} -I$${d}"; done; \
	extra_c=""; \
	if [ "$${alloc}" = "1" ] && [ -f "$(SMACKER_DIR)/alloc.c" ]; then \
		extra_c="$(SMACKER_DIR)/alloc.c"; \
	fi; \
	if [ -n "$${scanner_cc}" ] || [ "$${cxx}" = "1" ]; then \
		tmpdir=$$(mktemp -d); \
		plf=""; case "$${parser_c}" in *.c.inc) plf="-x c" ;; esac; \
		$(TS_CC) -fPIC -O2 -std=c11 $${iflags} $${plf} -c "$${parser_c}" -o "$${tmpdir}/parser.o" 2>&1 || \
			{ echo "  ✗ $${name}: parser.c failed"; rm -rf "$${tmpdir}"; exit 1; }; \
		$(TS_CXX) -fPIC -O2 -std=c++14 $${iflags} -c "$${scanner_cc}" -o "$${tmpdir}/scanner.o" 2>&1 || \
			{ echo "  ✗ $${name}: scanner.cc failed"; rm -rf "$${tmpdir}"; exit 1; }; \
		obj_files="$${tmpdir}/parser.o $${tmpdir}/scanner.o"; \
		if [ -n "$${extra_c}" ]; then \
			$(TS_CC) -fPIC -O2 -std=c11 $${iflags} -c "$${extra_c}" -o "$${tmpdir}/alloc.o" 2>&1 || \
				{ echo "  ✗ $${name}: alloc.c failed"; rm -rf "$${tmpdir}"; exit 1; }; \
			obj_files="$${obj_files} $${tmpdir}/alloc.o"; \
		fi; \
		$(TS_CXX) -shared -fPIC -o "$${output}" $${obj_files} 2>&1 || \
			{ echo "  ✗ $${name}: linking failed"; rm -rf "$${tmpdir}"; exit 1; }; \
		rm -rf "$${tmpdir}"; \
	else \
		cc_args=""; \
		for cf in $${parser_c} $${scanner_c} $${extra_c}; do \
			case "$${cf}" in *.c.inc) cc_args="$${cc_args} -x c $${cf} -x none" ;; \
			*) cc_args="$${cc_args} $${cf}" ;; esac; \
		done; \
		$(TS_CC) $(TS_CFLAGS) $${iflags} -o "$${output}" $${cc_args} 2>&1 || \
			{ echo "  ✗ $${name}: compilation failed"; exit 1; }; \
	fi; \
	size=$$(du -h "$${output}" | cut -f1); \
	echo "  ✓ $${name} ($${size})"
endef

grammars: grammars-treesitter grammars-antlr
	@echo ""
	@echo "  ✅ All grammars built."
	@echo "  Tree-sitter: $(TS_OUTDIR)/"
	@echo "  ANTLR:       $(ANTLR_OUTDIR)/"
	@echo ""

.PHONY: ensure-go-modules
ensure-go-modules:
	@go mod download

grammars-treesitter: ensure-go-modules
	@mkdir -p $(TS_OUTDIR)
	@echo ""
	@echo "═══════════════════════════════════════════════════════════════════════"
	@echo "  Building $(words $(TS_ALL)) tree-sitter grammars"
	@echo "═══════════════════════════════════════════════════════════════════════"
	@echo ""
	@echo "  Category A: smacker/go-tree-sitter ($(words $(TS_ALL_SMACKER)) grammars)"
	@echo "  ──────────────────────────────────────────────────────────────────"
	@# Simple smacker grammars (no alloc, no subdir).
	@for lang in $(TS_GRAMMARS_SMACKER_SIMPLE); do \
		$(call compile_ts_grammar,$${lang},$(SMACKER_DIR)/$${lang},$(SMACKER_DIR)/$${lang} $(SMACKER_DIR),,); \
	done
	@# Smacker grammars that need alloc.c.
	@for lang in $(TS_GRAMMARS_SMACKER_ALLOC); do \
		$(call compile_ts_grammar,$${lang},$(SMACKER_DIR)/$${lang},$(SMACKER_DIR)/$${lang} $(SMACKER_DIR),1,); \
	done
	@# Smacker grammars with non-standard subdirectories.
	@for lang in $(TS_GRAMMARS_SMACKER_SUBDIR); do \
		case $${lang} in \
		markdown)   subdir="markdown/tree-sitter-markdown" ;; \
		tsx)        subdir="typescript/tsx" ;; \
		typescript) subdir="typescript/typescript" ;; \
		esac; \
		$(call compile_ts_grammar,$${lang},$(SMACKER_DIR)/$${subdir},$(SMACKER_DIR)/$${subdir} $(SMACKER_DIR),,); \
	done
	@# Smacker grammars with C++ scanner.
	@for lang in $(TS_GRAMMARS_SMACKER_CXX); do \
		$(call compile_ts_grammar,$${lang},$(SMACKER_DIR)/$${lang},$(SMACKER_DIR)/$${lang} $(SMACKER_DIR),,1); \
	done
	@echo ""
	@echo "  Category B: External Go modules ($(words $(TS_ALL_EXTERNAL)) grammars)"
	@echo "  ──────────────────────────────────────────────────────────────────"
	@for spec in $(TS_GRAMMARS_EXTERNAL); do \
		lang=$$(echo "$$spec" | cut -d: -f1); \
		modspec=$$(echo "$$spec" | cut -d: -f2); \
		modpath=$$(echo "$$modspec" | sed 's|@.*||'); \
		version=$$(echo "$$modspec" | sed 's|.*/||; s|/.*||' | grep -oP '@.*' || echo "$$modspec" | grep -oP '@[^/]+'); \
		subdir=$$(echo "$$modspec" | sed -n 's|.*@[^/]*/||p'); \
		escaped=$$(echo "$$modpath$$version" | sed 's|/|/|g'); \
		moddir="$(GOMODCACHE)/$${escaped}"; \
		if [ ! -d "$$moddir" ]; then \
			go mod download "$$modpath$$version" 2>/dev/null || true; \
			moddir="$(GOMODCACHE)/$${escaped}"; \
		fi; \
		if [ ! -d "$$moddir" ]; then \
			moddir=$$(find "$(GOMODCACHE)/$$(dirname $$modpath)" -maxdepth 1 -name "$$(basename $$modpath)*" 2>/dev/null | head -1); \
		fi; \
		if [ -n "$$subdir" ]; then srcdir="$$moddir/$$subdir/src"; \
		else srcdir="$$moddir/src"; fi; \
		$(call compile_ts_grammar,$${lang},$${srcdir},$${srcdir},,); \
	done
	@echo ""
	@echo "  Category C: Local vendored ($(words $(TS_GRAMMARS_LOCAL)) grammars)"
	@echo "  ──────────────────────────────────────────────────────────────────"
	@for lang in $(TS_GRAMMARS_LOCAL); do \
		$(call compile_ts_grammar,$${lang},internal/ast/treesitter/$${lang},internal/ast/treesitter/$${lang},,); \
	done
	@echo ""
	@# Rename .so files where module dir name ≠ tree-sitter symbol name.
	@# DynGrammarLoader derives both filename and symbol from the YAML grammar field,
	@# so the .so name must match the symbol (tree_sitter_<name>).
	@for rename in golang:go csharp:c-sharp protobuf:proto; do \
		from=$$(echo "$$rename" | cut -d: -f1); \
		to=$$(echo "$$rename" | cut -d: -f2); \
		if [ -f "$(TS_OUTDIR)/tree-sitter-$${from}.so" ]; then \
			mv "$(TS_OUTDIR)/tree-sitter-$${from}.so" "$(TS_OUTDIR)/tree-sitter-$${to}.so"; \
		fi; \
		if [ -f "$(TS_OUTDIR)/tree-sitter-$${from}.dylib" ]; then \
			mv "$(TS_OUTDIR)/tree-sitter-$${from}.dylib" "$(TS_OUTDIR)/tree-sitter-$${to}.dylib"; \
		fi; \
		if [ -f "$(TS_OUTDIR)/tree-sitter-$${from}.dll" ]; then \
			mv "$(TS_OUTDIR)/tree-sitter-$${from}.dll" "$(TS_OUTDIR)/tree-sitter-$${to}.dll"; \
		fi; \
	done
	@echo ""
	@total=$$(ls -1 $(TS_OUTDIR)/*$(SHLIB_EXT) 2>/dev/null | wc -l); \
	totalsize=$$(du -sh $(TS_OUTDIR) | cut -f1); \
	echo "  Summary: $${total}/$(words $(TS_ALL)) grammars built ($${totalsize})"
	@echo ""

grammars-antlr:
	@mkdir -p $(ANTLR_OUTDIR)
	@echo ""
	@echo "═══════════════════════════════════════════════════════════════════════"
	@echo "  Building $(words $(ANTLR_GRAMMARS)) ANTLR sidecar binaries"
	@echo "═══════════════════════════════════════════════════════════════════════"
	@echo ""
	@success=0; failed=0; \
	for grammar in $(ANTLR_GRAMMARS); do \
		if go build -tags "$(BUILD_TAGS),grammar_$${grammar}" \
			-ldflags="-s -w" -trimpath \
			-o "$(ANTLR_OUTDIR)/antlr-sidecar-$${grammar}" \
			./cmd/graphit-antlr-sidecar/ 2>&1; then \
			size=$$(du -h "$(ANTLR_OUTDIR)/antlr-sidecar-$${grammar}" | cut -f1); \
			echo "  ✓ $${grammar} ($${size})"; \
			success=$$((success + 1)); \
		else \
			echo "  ✗ $${grammar}: build failed"; \
			failed=$$((failed + 1)); \
		fi; \
	done; \
	echo ""; \
	totalsize=$$(du -sh $(ANTLR_OUTDIR) 2>/dev/null | cut -f1); \
	echo "  Summary: $${success}/$(words $(ANTLR_GRAMMARS)) sidecars built ($${totalsize})"; \
	echo ""; \
	if [ "$$failed" -gt 0 ]; then exit 1; fi

grammars-clean:
	rm -rf $(TS_OUTDIR) $(ANTLR_OUTDIR)


# ═══════════════════════════════════════════════════════════════════════════════
# Bundle Macros (for launcher embed)
# ═══════════════════════════════════════════════════════════════════════════════

define bundle_model
	@mkdir -p cmd/launcher/runtime/models
	cp $(MODEL_CACHE)/model.onnx.gz cmd/launcher/runtime/models/model.onnx.gz
	cp $(MODEL_CACHE)/tokenizer.json cmd/launcher/runtime/models/tokenizer.json
endef

define bundle_ast
	@mkdir -p cmd/launcher/runtime/ast/queries
	@mkdir -p cmd/launcher/runtime/ast/frameworks
	cp internal/ast/queries/*.yaml cmd/launcher/runtime/ast/queries/
	cp internal/ast/frameworks/*.yaml cmd/launcher/runtime/ast/frameworks/
	cp internal/ast/ecosystems.yaml cmd/launcher/runtime/ast/
endef

define bundle_grammars
	@mkdir -p cmd/launcher/runtime/grammars/treesitter
	@touch cmd/launcher/runtime/grammars/treesitter/KEEP
	@for lang in $(DEFAULT_TS_GRAMMARS); do \
		for candidate in \
			$(TS_OUTDIR)/tree-sitter-$${lang}.so \
			$(TS_OUTDIR)/tree-sitter-$${lang}.dylib \
			$(TS_OUTDIR)/tree-sitter-$${lang}.dll; do \
			if [ -f "$$candidate" ]; then \
				cp "$$candidate" cmd/launcher/runtime/grammars/treesitter/; \
				break; \
			fi; \
		done; \
	done
	@count=$$(ls -1 cmd/launcher/runtime/grammars/treesitter/*.so cmd/launcher/runtime/grammars/treesitter/*.dylib cmd/launcher/runtime/grammars/treesitter/*.dll 2>/dev/null | wc -l); \
	if [ "$$count" -eq 0 ]; then \
		echo "  ✗ FATAL: No grammars were bundled! Run 'make grammars-treesitter' first."; \
		exit 1; \
	else \
		echo "  ✓ Bundled $$count grammar(s)"; \
	fi
endef

define bundle_antlr
	@if [ -n "$(DEFAULT_ANTLR_GRAMMARS)" ]; then \
		mkdir -p cmd/launcher/runtime/grammars/antlr; \
		for grammar in $(DEFAULT_ANTLR_GRAMMARS); do \
			for candidate in \
				$(ANTLR_OUTDIR)/antlr-sidecar-$${grammar} \
				$(ANTLR_OUTDIR)/antlr-sidecar-$${grammar}.exe; do \
				if [ -f "$$candidate" ]; then \
					cp "$$candidate" cmd/launcher/runtime/grammars/antlr/; \
					break; \
				fi; \
			done; \
		done; \
		count=$$(ls -1 cmd/launcher/runtime/grammars/antlr/antlr-sidecar-* 2>/dev/null | wc -l); \
		if [ "$$count" -eq 0 ]; then \
			echo "  ✗ FATAL: No ANTLR sidecars were bundled! Run 'make grammars-antlr' first."; \
			exit 1; \
		else \
			echo "  ✓ Bundled $$count ANTLR sidecar(s)"; \
		fi; \
	fi
endef


# ═══════════════════════════════════════════════════════════════════════════════
# UI
# ═══════════════════════════════════════════════════════════════════════════════

ui:
	cd internal/ui && npm ci --prefer-offline
	cd internal/ui && npm run build

ui-dev:
	cd internal/ui && npm run dev


# ═══════════════════════════════════════════════════════════════════════════════
# Dependencies
# ═══════════════════════════════════════════════════════════════════════════════

setup-lbug:
	@go mod download github.com/LadybugDB/go-ladybug
	@chmod -R u+w "$(LBUG_MOD)" 2>/dev/null || true
	@mkdir -p $(LBUG_CACHE)
	@for plat in $(LBUG_PLATFORMS); do \
		case $$plat in \
		linux-amd64) \
			if [ ! -f "$(LBUG_MOD)/lib/dynamic/linux-amd64/liblbug.so" ]; then \
				echo "  → Downloading liblbug for linux-x86_64…"; \
				mkdir -p "$(LBUG_MOD)/lib/dynamic/linux-amd64"; \
				curl -sSL "https://github.com/LadybugDB/ladybug/releases/latest/download/liblbug-linux-x86_64.tar.gz" -o "$(LBUG_CACHE)/liblbug-linux-x86_64.tar.gz"; \
				rm -rf "$(LBUG_CACHE)/linux-amd64" && mkdir -p "$(LBUG_CACHE)/linux-amd64"; \
				tar xzf "$(LBUG_CACHE)/liblbug-linux-x86_64.tar.gz" -C "$(LBUG_CACHE)/linux-amd64"; \
				find "$(LBUG_CACHE)/linux-amd64" -name "liblbug.so" -exec cp -L {} "$(LBUG_MOD)/lib/dynamic/linux-amd64/" \; ; \
			fi ;; \
		darwin) \
			if [ ! -f "$(LBUG_MOD)/lib/dynamic/darwin/liblbug.dylib" ]; then \
				echo "  → Downloading liblbug for darwin (arm64 + x86_64)…"; \
				mkdir -p "$(LBUG_MOD)/lib/dynamic/darwin"; \
				curl -sSL "https://github.com/LadybugDB/ladybug/releases/latest/download/liblbug-osx-arm64.tar.gz" -o "$(LBUG_CACHE)/liblbug-osx-arm64.tar.gz"; \
				curl -sSL "https://github.com/LadybugDB/ladybug/releases/latest/download/liblbug-osx-x86_64.tar.gz" -o "$(LBUG_CACHE)/liblbug-osx-x86_64.tar.gz"; \
				rm -rf "$(LBUG_CACHE)/darwin-arm64" "$(LBUG_CACHE)/darwin-x86_64"; \
				mkdir -p "$(LBUG_CACHE)/darwin-arm64" "$(LBUG_CACHE)/darwin-x86_64"; \
				tar xzf "$(LBUG_CACHE)/liblbug-osx-arm64.tar.gz" -C "$(LBUG_CACHE)/darwin-arm64"; \
				tar xzf "$(LBUG_CACHE)/liblbug-osx-x86_64.tar.gz" -C "$(LBUG_CACHE)/darwin-x86_64"; \
				if command -v lipo >/dev/null 2>&1; then \
					lipo -create "$(LBUG_CACHE)/darwin-arm64/liblbug.dylib" "$(LBUG_CACHE)/darwin-x86_64/liblbug.dylib" \
						-output "$(LBUG_MOD)/lib/dynamic/darwin/liblbug.dylib"; \
				else \
					cp -L "$(LBUG_CACHE)/darwin-x86_64/liblbug.dylib" "$(LBUG_MOD)/lib/dynamic/darwin/liblbug.dylib"; \
				fi; \
			fi ;; \
		windows) \
			if [ ! -f "$(LBUG_MOD)/lib/dynamic/windows/lbug_shared.dll" ]; then \
				echo "  → Downloading liblbug for windows-x86_64…"; \
				mkdir -p "$(LBUG_MOD)/lib/dynamic/windows"; \
				curl -sSL "https://github.com/LadybugDB/ladybug/releases/latest/download/liblbug-windows-x86_64.zip" -o "$(LBUG_CACHE)/liblbug-windows-x86_64.zip"; \
				rm -rf "$(LBUG_CACHE)/windows" && mkdir -p "$(LBUG_CACHE)/windows"; \
				unzip -qo "$(LBUG_CACHE)/liblbug-windows-x86_64.zip" -d "$(LBUG_CACHE)/windows"; \
				find "$(LBUG_CACHE)/windows" -name "lbug_shared.dll" -exec cp -L {} "$(LBUG_MOD)/lib/dynamic/windows/" \; ; \
			fi ;; \
		*) echo "  ⚠ Unknown platform: $$plat" ;; \
		esac; \
	done

fetch-model:
	@mkdir -p $(MODEL_CACHE)
	@if [ ! -f $(MODEL_CACHE)/model.onnx ]; then \
		echo "→ Downloading CodeRankEmbed-137M INT8 model (~132MB)…"; \
		curl -sSL "https://huggingface.co/$(MODEL_REPO)/resolve/main/onnx/model.onnx" -o $(MODEL_CACHE)/model.onnx.tmp; \
		mv $(MODEL_CACHE)/model.onnx.tmp $(MODEL_CACHE)/model.onnx; \
	fi
	@if [ ! -f $(MODEL_CACHE)/tokenizer.json ]; then \
		echo "→ Downloading CodeRankEmbed tokenizer…"; \
		curl -sSL "https://huggingface.co/$(MODEL_REPO)/resolve/main/tokenizer.json" -o $(MODEL_CACHE)/tokenizer.json.tmp; \
		mv $(MODEL_CACHE)/tokenizer.json.tmp $(MODEL_CACHE)/tokenizer.json; \
	fi
	@if [ ! -f $(MODEL_CACHE)/model.onnx.gz ] || [ $(MODEL_CACHE)/model.onnx -nt $(MODEL_CACHE)/model.onnx.gz ]; then \
		echo "→ Compressing model.onnx with gzip…"; \
		gzip -9 -c $(MODEL_CACHE)/model.onnx > $(MODEL_CACHE)/model.onnx.gz; \
	fi

fetch-ort-linux:
	@mkdir -p $(ORT_CACHE)
	@if [ ! -f $(ORT_CACHE)/onnxruntime-linux-x64-$(ORT_VERSION)/lib/libonnxruntime.so ]; then \
		echo "→ Downloading ONNX Runtime $(ORT_VERSION) for linux-x64…"; \
		curl -sSL "https://github.com/microsoft/onnxruntime/releases/download/v$(ORT_VERSION)/onnxruntime-linux-x64-$(ORT_VERSION).tgz" -o $(ORT_CACHE)/ort.tgz; \
		cd $(ORT_CACHE) && tar xzf ort.tgz; \
	fi

fetch-ort-darwin:
	@mkdir -p $(ORT_CACHE)
	@if [ ! -f $(ORT_CACHE)/onnxruntime-osx-arm64-$(ORT_VERSION)/lib/libonnxruntime.dylib ]; then \
		echo "→ Downloading ONNX Runtime $(ORT_VERSION) for darwin-arm64…"; \
		curl -sSL "https://github.com/microsoft/onnxruntime/releases/download/v$(ORT_VERSION)/onnxruntime-osx-arm64-$(ORT_VERSION).tgz" -o $(ORT_CACHE)/ort-darwin-arm64.tgz; \
		cd $(ORT_CACHE) && tar xzf ort-darwin-arm64.tgz; \
	fi

fetch-ort-windows:
	@mkdir -p $(ORT_CACHE)
	@if [ ! -f $(ORT_CACHE)/onnxruntime-win-x64-$(ORT_VERSION)/lib/onnxruntime.dll ]; then \
		echo "→ Downloading ONNX Runtime $(ORT_VERSION) for windows-x64…"; \
		curl -sSL "https://github.com/microsoft/onnxruntime/releases/download/v$(ORT_VERSION)/onnxruntime-win-x64-$(ORT_VERSION).zip" -o $(ORT_CACHE)/ort-win-x64.zip; \
		cd $(ORT_CACHE) && unzip -qo ort-win-x64.zip; \
	fi


# ═══════════════════════════════════════════════════════════════════════════════
# Build Targets
# ═══════════════════════════════════════════════════════════════════════════════

build: build-linux

install: build
	sudo cp $(BIN_DIR)/$(BRAND)-linux-amd64 /usr/local/bin/$(BRAND)

build-linux: ui setup-lbug fetch-ort-linux fetch-model $(GRAMMAR_DEPS) $(ANTLR_DEPS)
	@mkdir -p cmd/launcher/runtime
	GOOS=linux GOARCH=amd64 CGO_ENABLED=1 go build -tags "$(BUILD_TAGS)" -ldflags "$(LDFLAGS)" -o cmd/launcher/runtime/$(BRAND)-core $(CMD)
	GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp ./cmd/mcp
	find $$(go env GOPATH)/pkg/mod/github.com/!ladybug!d!b/go-ladybug*/lib/dynamic/linux-amd64 -name "liblbug.so" -exec cp -L {} cmd/launcher/runtime/ \;
	cd cmd/launcher/runtime && cp liblbug.so liblbug.so.0
	find /usr/lib /lib -name "libicu*.so.[0-9]*" -exec cp -L {} cmd/launcher/runtime/ \; 2>/dev/null || true
	rm -f cmd/launcher/runtime/*.so.*.*
	cp -L $(ORT_CACHE)/onnxruntime-linux-x64-$(ORT_VERSION)/lib/libonnxruntime.so cmd/launcher/runtime/
	$(call bundle_model)
	$(call bundle_ast)
	$(if $(SKIP_GRAMMARS),,$(call bundle_grammars))
	$(if $(SKIP_ANTLR_GRAMMARS),,$(call bundle_antlr))
	@mkdir -p $(BIN_DIR)
	GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS)" -o $(BIN_DIR)/$(BRAND)-linux-amd64 ./cmd/launcher
	rm -rf cmd/launcher/runtime/*

build-darwin: ui setup-lbug fetch-ort-darwin fetch-model $(GRAMMAR_DEPS) $(ANTLR_DEPS)
	@mkdir -p cmd/launcher/runtime
	GOOS=darwin GOARCH=arm64 CGO_ENABLED=1 go build -tags "$(BUILD_TAGS)" -ldflags "$(LDFLAGS)" -o cmd/launcher/runtime/$(BRAND)-core $(CMD)
	GOOS=darwin GOARCH=arm64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp ./cmd/mcp
	find $$(go env GOPATH)/pkg/mod/github.com/!ladybug!d!b/go-ladybug*/lib/dynamic/darwin -name "liblbug.dylib" -exec cp -L {} cmd/launcher/runtime/ \;
	cd cmd/launcher/runtime && cp liblbug.dylib liblbug.0.dylib
	find /opt/homebrew/opt/icu4c/lib /usr/local/opt/icu4c/lib -name "libicu*.[0-9]*.dylib" -exec cp -L {} cmd/launcher/runtime/ \; 2>/dev/null || true
	rm -f cmd/launcher/runtime/*.*.*.dylib
	cp -L $(ORT_CACHE)/onnxruntime-osx-arm64-$(ORT_VERSION)/lib/libonnxruntime.dylib cmd/launcher/runtime/
	$(call bundle_model)
	$(call bundle_ast)
	$(if $(SKIP_GRAMMARS),,$(call bundle_grammars))
	$(if $(SKIP_ANTLR_GRAMMARS),,$(call bundle_antlr))
	@mkdir -p $(BIN_DIR)
	GOOS=darwin GOARCH=arm64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS)" -o $(BIN_DIR)/$(BRAND)-darwin-arm64 ./cmd/launcher
	rm -rf cmd/launcher/runtime/*

build-windows: ui setup-lbug fetch-ort-windows fetch-model $(GRAMMAR_DEPS) $(ANTLR_DEPS)
	@mkdir -p cmd/launcher/runtime
	CC=x86_64-w64-mingw32-gcc CXX=x86_64-w64-mingw32-g++ CGO_CFLAGS="-I/usr/x86_64-w64-mingw32/icu/include -I/usr/include" CGO_CXXFLAGS="-I/usr/x86_64-w64-mingw32/icu/include -I/usr/include" CGO_LDFLAGS="-L/usr/x86_64-w64-mingw32/icu/lib -licuuc -licuin -licudt -lstdc++ -static-libgcc -static-libstdc++" GOOS=windows GOARCH=amd64 CGO_ENABLED=1 go build -tags "$(BUILD_TAGS)" -ldflags "$(LDFLAGS)" -o cmd/launcher/runtime/$(BRAND)-core.exe $(CMD)
	GOOS=windows GOARCH=amd64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp.exe ./cmd/mcp
	find $$(go env GOPATH)/pkg/mod/github.com/!ladybug!d!b/go-ladybug*/lib/dynamic/windows -name "lbug_shared.dll" -exec cp -L {} cmd/launcher/runtime/ \;
	find /usr/x86_64-w64-mingw32 -name "*.dll" -exec cp -L {} cmd/launcher/runtime/ \; 2>/dev/null || true
	cp -L $(ORT_CACHE)/onnxruntime-win-x64-$(ORT_VERSION)/lib/onnxruntime.dll cmd/launcher/runtime/
	$(call bundle_model)
	$(call bundle_ast)
	$(if $(SKIP_GRAMMARS),,$(call bundle_grammars))
	$(if $(SKIP_ANTLR_GRAMMARS),,$(call bundle_antlr))
	@mkdir -p $(BIN_DIR)
	GOOS=windows GOARCH=amd64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS)" -o $(BIN_DIR)/$(BRAND)-windows-amd64.exe ./cmd/launcher
	rm -rf cmd/launcher/runtime/*

build-windows-native: ui setup-lbug fetch-ort-windows fetch-model $(GRAMMAR_DEPS) $(ANTLR_DEPS)
	@mkdir -p cmd/launcher/runtime
	CGO_ENABLED=1 CGO_CFLAGS="-I/mingw64/include" CGO_CXXFLAGS="-I/mingw64/include" CGO_LDFLAGS="-L/mingw64/lib -licuuc -licuin -licudt -lstdc++" go build -tags "$(BUILD_TAGS)" -ldflags "$(LDFLAGS)" -o cmd/launcher/runtime/$(BRAND)-core.exe $(CMD)
	CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp.exe ./cmd/mcp
	GOPATH_UNIX=$$(cygpath -u "$$(go env GOPATH)") && find $$GOPATH_UNIX/pkg/mod/github.com/!ladybug!d!b/go-ladybug*/lib/dynamic/windows -name "lbug_shared.dll" -exec cp -L {} cmd/launcher/runtime/ \;
	cp /mingw64/bin/libicuuc*.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp /mingw64/bin/libicuin*.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp /mingw64/bin/libicudt*.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp /mingw64/bin/libgcc_s_seh-1.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp /mingw64/bin/libstdc++-6.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp /mingw64/bin/libwinpthread-1.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp -L $(ORT_CACHE)/onnxruntime-win-x64-$(ORT_VERSION)/lib/onnxruntime.dll cmd/launcher/runtime/
	$(call bundle_model)
	$(call bundle_ast)
	$(if $(SKIP_GRAMMARS),,$(call bundle_grammars))
	$(if $(SKIP_ANTLR_GRAMMARS),,$(call bundle_antlr))
	@mkdir -p $(BIN_DIR)
	CGO_ENABLED=0 go build -ldflags "$(LDFLAGS)" -o $(BIN_DIR)/$(BRAND)-windows-amd64.exe ./cmd/launcher
	rm -rf cmd/launcher/runtime/*

build-all: build-linux build-darwin build-windows


# ═══════════════════════════════════════════════════════════════════════════════
# Quality
# ═══════════════════════════════════════════════════════════════════════════════

test: setup-lbug
	@LBUG_LIB="$(LBUG_MOD)/lib/dynamic/linux-amd64"; \
	if [ -f "$$LBUG_LIB/liblbug.so" ] && [ ! -f "$$LBUG_LIB/liblbug.so.0" ]; then \
		cp -L "$$LBUG_LIB/liblbug.so" "$$LBUG_LIB/liblbug.so.0"; \
	fi; \
	GRAMMAR_DIR="$$(pwd)/$(TS_OUTDIR)"; \
	echo "  → Running tests with race detector (project code)…"; \
	GRAPHIT_GRAMMAR_DIR="$$GRAMMAR_DIR" LD_LIBRARY_PATH="$$LBUG_LIB:$$LD_LIBRARY_PATH" go test -race -cover -p 4 \
		$$(go list ./... | grep -v "/antlr/" | grep -v "/treesitter/"); \
	echo "  → Running tests without race detector (generated parsers)…"; \
	GRAPHIT_GRAMMAR_DIR="$$GRAMMAR_DIR" LD_LIBRARY_PATH="$$LBUG_LIB:$$LD_LIBRARY_PATH" go test -cover -p 4 \
		$$(go list ./... | grep -E "/antlr/|/treesitter/")

lint:
	golangci-lint run ./...

vulncheck:
	go run golang.org/x/vuln/cmd/govulncheck@latest ./...

ui-lint:
	cd internal/ui && npm run lint

fmt:
	gofmt -w .

vet:
	go vet $$(go list ./... | grep -v "/antlr/plsql" | grep -v "/antlr/postgresql" | grep -v "/antlr/tsql" | grep -v "/antlr/db2" | grep -v "/antlr/cobol85")

# ── CI reproduce (matches .github/workflows/ci.yml) ──────────────────────────
# Run all checks that GitHub Actions runs, in the same order.
# Usage: make ci
ci: vet lint vulncheck test ui ui-lint
	@echo ""
	@echo "  ✅ All CI checks passed."
	@echo ""

# ── Quick pre-push check (no build, no UI) ───────────────────────────────────
# Usage: make check
check: vet lint vulncheck test
	@echo ""
	@echo "  ✅ Go checks passed (vet + lint + vulncheck + test)."
	@echo ""

clean:
	rm -rf $(BIN_DIR)
	rm -rf internal/ui/dist

run:
	go run $(CMD) $(ARGS)

update-deps:
	go get -u ./...
	go mod tidy
