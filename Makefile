.PHONY: build build-all build-local install install-darwin install-windows clean fmt vet run ui ui-dev setup-lbug fetch-lancedb lancedb-native build-local lancedb-cgo-env \
       fetch-ort-linux fetch-ort-darwin fetch-ort-windows lint \
       ui-lint ci check test build-windows-native \
       grammars grammars-treesitter grammars-antlr grammars-clean

MODULE   := github.com/graphit-labs/graphit-code
CMD      := ./cmd/graphit
BIN_DIR  := .build

BRAND        ?= graphit
# The env-var prefix, mirroring brand.EnvPrefix() in Go.
BRAND_ENV    := $(shell echo $(BRAND) | tr '[:lower:]' '[:upper:]')
DISPLAY_NAME ?= Graphit Code: A Powerful Agent Harness for Enterprise Software Ecosystems
VERSION      ?= dev
GITHUB_REPO  ?= graphit-labs/graphit-code
DEFAULT_HUB_BUCKET   ?=
DEFAULT_HUB_REGION   ?=
DEFAULT_HUB_ENDPOINT ?=
SELF_UPDATE_URL ?=
COMPILE_CONFIG ?=
# Install directory for Linux/macOS (override: make install PREFIX=$$HOME/.local/bin)
PREFIX       ?= /usr/local/bin
# Install directory for Windows/MSYS2 (override: make install-windows PREFIX_WIN='C:\Tools\graphit')
PREFIX_WIN   ?=

ifeq ($(OS),Windows_NT)
  BUILD_ID ?= $(shell powershell -Command "[System.Guid]::NewGuid().ToString()")
else
  BUILD_ID ?= $(shell cat /proc/sys/kernel/random/uuid 2>/dev/null || uuidgen)
endif

LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.Brand=$(BRAND)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.DisplayName=$(DISPLAY_NAME)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/version.Version=$(VERSION)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/version.BuildID=$(BUILD_ID)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.GitHubRepo=$(GITHUB_REPO)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.DefaultHubBucket=$(DEFAULT_HUB_BUCKET)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.DefaultHubRegion=$(DEFAULT_HUB_REGION)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.DefaultHubEndpoint=$(DEFAULT_HUB_ENDPOINT)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.SelfUpdateURL=$(SELF_UPDATE_URL)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/config.CompiledDefaults=$(COMPILE_CONFIG)'

# SQLITE IS GONE, and with it the `fts5` tag that existed only to compile FTS5 into
# go-sqlite3. Search is LanceDB and nothing else, which is why `lancedb` is not optional:
# a binary built without it links the ErrNotBuilt stubs and has no search at all.
BUILD_TAGS := lancedb

# LOCAL_TAGS is what the local loop uses.
LOCAL_TAGS := $(BUILD_TAGS)


# Where the LanceDB native lives for LOCAL builds and tests.
#
# NOT cmd/launcher/runtime — that is the launcher's staging area and `build-linux` ends with
# `rm -rf cmd/launcher/runtime/*`, so a release build would silently break the next `go test`.
# The link path is compiled into internal/lancestore/cgo_lancedb.go via ${SRCDIR}, so this
# directory name is part of that contract; changing one without the other breaks the link.
# `lancedb-native` populates it automatically: existing copy → symlink into the extracted dev
# runtime → cargo build, in that order.
LANCEDB_LIB_DIR := .native

# The library name is mapped with MAKE conditionals and not with a shell `case`, because make
# does not parse shell syntax: inside `$(shell ...)` the first unbalanced `)` — which `case`
# arms are made of — closes the function call and silently truncates the value. That produced
# a path ending in `.native/ echo liblancedb_go.dylib ;; ...`, which never exists, so the
# guard rebuilt the native on every single invocation.
LANCEDB_GOOS := $(shell go env GOOS)
ifeq ($(LANCEDB_GOOS),darwin)
LANCEDB_LIB_NAME := liblancedb_go.dylib
else ifeq ($(LANCEDB_GOOS),windows)
LANCEDB_LIB_NAME := lancedb_go.dll
else
LANCEDB_LIB_NAME := liblancedb_go.so
endif
LANCEDB_LIB := $(LANCEDB_LIB_DIR)/$(LANCEDB_LIB_NAME)

# The generated parsers are excluded from the race detector run and get their own
# pass in `test`; node_modules is excluded from both.
#
# `make ui` runs npm ci, and one of the UI's transitive packages ships Go sources
# under internal/ui/node_modules — so after a UI build `go list ./...` starts
# returning a package that is not ours. `make ci` runs ui first, which means vet,
# lint and test would all cover third-party code that the GitHub jobs never see:
# they only create a dist placeholder, so node_modules does not exist there.
# .golangci.yml already excludes the directory for this reason; these two keep the
# go tool consistent with it.
GO_PKGS_SKIP    := /antlr/|/treesitter/|/node_modules/
GO_PKGS_PARSERS := /antlr/|/treesitter/

# Pinned so the check is reproducible and cacheable. See the `vulncheck` target for why
# @latest is not an option, and the `security` job in .github/workflows/ci.yml, which runs
# `make vulncheck` rather than declaring its own version.
GOVULNCHECK_VERSION := v1.7.0
ACTIONLINT_VERSION  := v1.7.7

# `go test -p N` builds and runs N packages at a time. It was hardcoded to 4, which is
# correct on a GitHub runner and wasteful on a workstation: on 20 cores it left 16 idle
# while internal/ast (106 s of execution, on top of linking 1.29M lines of generated ANTLR)
# ran alone. Capped at 8 rather than nproc because the race detector multiplies the memory
# each concurrent test binary holds, and the cap is what keeps a 20-core machine from
# swapping. Override on the command line when you know better: `make test GO_TEST_P=16`.
NPROC     := $(shell getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)
GO_TEST_P ?= $(shell if [ "$(NPROC)" -gt 8 ]; then echo 8; else echo "$(NPROC)"; fi)

# Must satisfy the ORT_API_VERSION the onnxruntime_go binding in go.mod compiles
# against (v1.31.0 declares 26). A runtime older than that aborts at
# InitializeEnvironment with "requested API version [26] is not available", which
# leaves the embedder nil and degrades semantic search to FTS-only in silence.
# Bump this together with the binding.
ORT_VERSION  := 1.26.0
ORT_CACHE    := /tmp/onnxruntime-cache

# The HOST's ONNX Runtime, for `make test`.
#
# The library is only ever CO-LOCATED WITH THE BINARY inside the launcher payload, so nothing
# built from this tree finds it: a `go test` binary lives in a temp directory the toolchain
# made, and findORTLibrary's next stop is the loader path, which nothing was setting. The
# consequence was invisible rather than loud — every test that needs an embedder called
# t.Skip, so TestHybridSearchQualityFloor reported success for months without running.
#
# What it was hiding, measured the first time it ran: the hybrid channel answered 0 of its 11
# decisive probes. See the comment on that test.
# The embedding model cache, shared by every test binary.
#
# Without this each test binary resolves the cache from its own throwaway HOME and downloads the
# ~132 MB model again — measured, 29 abandoned copies holding 4.3 GB of tmpfs. One shared
# directory outside the operator's home is the same trade ORT_CACHE and LANCEDB_CACHE already
# make. Overridable so CI can point it at a path it knows how to cache between runs.
MODEL_CACHE ?= /tmp/$(BRAND)-model-cache

ORT_HOST_GOOS := $(shell go env GOOS)
ifeq ($(ORT_HOST_GOOS),darwin)
ORT_HOST_FETCH := fetch-ort-darwin
ORT_HOST_LIB   := $(ORT_CACHE)/onnxruntime-osx-arm64-$(ORT_VERSION)/lib
else ifeq ($(ORT_HOST_GOOS),windows)
ORT_HOST_FETCH := fetch-ort-windows
ORT_HOST_LIB   := $(ORT_CACHE)/onnxruntime-win-x64-$(ORT_VERSION)/lib
else
ORT_HOST_FETCH := fetch-ort-linux
ORT_HOST_LIB   := $(ORT_CACHE)/onnxruntime-linux-x64-$(ORT_VERSION)/lib
endif

# STRIP_LDFLAGS omits the DWARF tables and the symbol table used by external
# debuggers from the core binary. MEASURED on linux/amd64: 257.6 MiB -> 192.9 MiB,
# a 25.1% cut, and 27.3 MiB off the compressed release artifact.
#
# It costs no runtime capability. -s -w does not touch the runtime's pclntab, so
# panic stack traces keep their function names and file:line, and
# runtime.Caller, runtime.FuncForPC, debug.Stack and debug.ReadBuildInfo all
# behave identically — verified by diffing the output of a program built both
# ways. The -X values above survive too. What is lost is source-level debugging
# of a released binary with delve or gdb, and `go tool nm` on the artifact.
#
# Override to keep a debuggable build: make build-linux STRIP_LDFLAGS=
STRIP_LDFLAGS ?= -s -w

# The embedding model is NOT built into the binary. It is ~132 MB, which is more
# than everything else the launcher carries put together, and it changes on a
# different schedule from the code. `graphit setup` downloads it once into
# ~/.<brand>/models/coderankembed/, and ModelManager fetches it on first use if
# setup could not. So there is no fetch-model target and no model in the build.

# ── LanceDB native ────────────────────────────────────────────────────────────
#
# The Go module and the native library MUST come from this one commit. They are two
# halves of one cgo boundary, and a mismatch does not fail the build — it fails at
# runtime, inside the FFI. So the SHA is pinned here AND in go.mod, and both move together.
#
# Why a commit and not a release: the only published release, v0.1.2 (2025-09-30),
# DOES NOT IMPLEMENT full-text search at all. MEASURED — the index is created and the
# query returns "Full-text search is not currently supported", because the binding's own
# rust/src/query.rs carries a `// placeholder for future implementation`. FTS, the RRF
# reranker and hybrid vector+FTS landed on main in April 2026 (#31, #32, #33) and no
# release has been cut since. The published artifact also ships 3 platforms while its
# release notes promise 5, which building per platform fixes for free.
LANCEDB_SHA   := fa14ce29c7724354f2cea630a1d3488b56bbd64b
LANCEDB_REPO  := https://github.com/lancedb/lancedb-go.git
# Overridable so CI can point it at a path it knows how to cache between runs.
LANCEDB_CACHE ?= /tmp/lancedb-native-cache
# Where the launcher extracts the dev runtime — the same machine-global location embedding_local.go
# resolves libonnxruntime from. lancedb-native links against a copy found here before considering a
# cargo build, so one install serves every checkout. Follows the GRAPHIT_GLOBAL_DIR override.
LANCEDB_RUNTIME_SOURCE ?= $(if $($(BRAND_ENV)_GLOBAL_DIR),$($(BRAND_ENV)_GLOBAL_DIR),$(HOME)/.$(BRAND))/runtime/dev

# The toolchain is pinned because UPSTREAM DOES NOT PIN ONE, and its committed Cargo.lock
# has already rotted: it holds ethnum 1.5.2, which fails on newer rustc with
# "E0512: cannot transmute between types of different sizes" (() is 0 bits,
# TryFromIntError is 8). ethnum is three levels down — jsonb, pulled by lance-arrow,
# lance-datafusion and lance-index — so it is nothing this project chose. The bump below
# is the whole fix; keep it and the toolchain pinned together, and re-verify both when
# moving LANCEDB_SHA.
LANCEDB_RUST     := 1.98.0
LANCEDB_ETHNUM   := 1.5.3

# The `aws` feature is enabled locally, and WITHOUT IT THERE IS NO ON-THE-FLY QUERY AT ALL.
# The binding depends on lancedb with default-features = false, and the crate declares
# default = [], so object-store support is compiled out — including S3. MEASURED: connecting
# to s3://… fails with "No object store provider found for scheme: 's3'. Valid schemes:
# file". This is true of the PUBLISHED artifacts too, since they are built from the same
# manifest, so no released native could ever have served a remote context. Proven against
# MinIO once enabled: table created on s3://, FTS index built on s3://, and hybrid
# vector+FTS answered with the engine's RRF — nothing downloaded.
#
# cdylib is re-enabled locally. Upstream set crate-type = ["staticlib"] in the pinned
# commit ("drop unused cdylib"), and a Rust staticlib does not carry its transitive C
# dependencies — linking it needs -lbz2 and friends spelled out per platform, which is a
# list this project would then own. The shared object resolves them itself, so the core
# binary links against it and the launcher puts it on the library path exactly as it
# already does for libonnxruntime. MEASURED: 8.9 MiB core against a 217 MiB .so, versus a
# 260 MiB core with the static link.

LBUG_VERSION := v0.17.0

# The extension server publishes per ENGINE version, which is NOT the go-ladybug module
# version: go-ladybug v0.17.0 ships liblbug 0.18.2, and the server has no 0.18.2 build —
# v0.18.1 is the newest published. MEASURED (internal/ladybugstore/httpfs_probe_test.go):
# the 0.18.1 binary loads on the 0.18.2 runtime and show_loaded_extensions() confirms it.
# Bump only after checking the URL returns 200 for the newer version.
LBUG_EXT_VERSION := 0.18.1
LBUG_EXT_HOST    := https://extension.ladybugdb.com
LBUG_EXT_CACHE   := /tmp/lbug-extension-cache
LBUG_MOD     := $(shell go env GOPATH)/pkg/mod/github.com/!ladybug!d!b/go-ladybug@$(LBUG_VERSION)
LBUG_CACHE   := /tmp/lbug-cache

LBUG_PLATFORMS ?= $(shell uname -s | sed 's/Darwin/darwin/;s/Linux/linux-amd64/;s/MINGW.*/windows/')

# ═══════════════════════════════════════════════════════════════════════════════
# Grammar Build (for Hub distribution only — defaults are compiled natively)
# ═══════════════════════════════════════════════════════════════════════════════


TS_OUTDIR     := .build/grammars/treesitter
ANTLR_OUTDIR  := .build/grammars/antlr


ifeq ($(OS),Windows_NT)
  GOMODCACHE    := $(shell cygpath -u "$$(go env GOMODCACHE)")
else
  GOMODCACHE    := $(shell go env GOMODCACHE)
endif


ifeq ($(OS),Windows_NT)
  SHLIB_EXT := .dll
else ifeq ($(shell uname -s),Darwin)
  SHLIB_EXT := .dylib
else
  SHLIB_EXT := .so
endif


TS_CC         := $(CC)
TS_CXX        := $(CXX)
TS_CFLAGS     := -shared -fPIC -O2 -std=c11 \
    -Dts_current_malloc=malloc -Dts_current_free=free \
    -Dts_current_realloc=realloc -Dts_current_calloc=calloc
TS_CXXFLAGS   := -shared -fPIC -O2 -std=c++14 \
    -Dts_current_malloc=malloc -Dts_current_free=free \
    -Dts_current_realloc=realloc -Dts_current_calloc=calloc


# Grammars sourced from Go modules that ship generated parser.c under bindings/go.
# Format: key:modulepath[:subdir]  (subdir for multi-grammar modules like php/xml/typescript).
GRAMMAR_MODULES := \
    bash:github.com/tree-sitter/tree-sitter-bash \
    c:github.com/tree-sitter/tree-sitter-c \
    cpp:github.com/tree-sitter/tree-sitter-cpp \
    c-sharp:github.com/tree-sitter/tree-sitter-c-sharp \
    css:github.com/tree-sitter/tree-sitter-css \
    go:github.com/tree-sitter/tree-sitter-go \
    haskell:github.com/tree-sitter/tree-sitter-haskell \
    html:github.com/tree-sitter/tree-sitter-html \
    java:github.com/tree-sitter/tree-sitter-java \
    javascript:github.com/tree-sitter/tree-sitter-javascript \
    json:github.com/tree-sitter/tree-sitter-json \
    julia:github.com/tree-sitter/tree-sitter-julia \
    php:github.com/tree-sitter/tree-sitter-php:php \
    python:github.com/tree-sitter/tree-sitter-python \
    ruby:github.com/tree-sitter/tree-sitter-ruby \
    rust:github.com/tree-sitter/tree-sitter-rust \
    scala:github.com/tree-sitter/tree-sitter-scala \
    typescript:github.com/tree-sitter/tree-sitter-typescript:typescript \
    tsx:github.com/tree-sitter/tree-sitter-typescript:tsx \
    hcl:github.com/tree-sitter-grammars/tree-sitter-hcl \
    lua:github.com/tree-sitter-grammars/tree-sitter-lua \
    toml:github.com/tree-sitter-grammars/tree-sitter-toml \
    xml:github.com/tree-sitter-grammars/tree-sitter-xml:xml \
    yaml:github.com/tree-sitter-grammars/tree-sitter-yaml \
    zig:github.com/tree-sitter-grammars/tree-sitter-zig


# Grammars vendored in-repo (parser.c.inc under internal/ast/treesitter/<key>).
GRAMMAR_VENDORED := clojure dockerfile elixir graphql groovy kotlin markdown \
    objc proto r sql svelte swift dart vue


TS_ALL := $(foreach spec,$(GRAMMAR_MODULES),$(firstword $(subst :, ,$(spec)))) $(GRAMMAR_VENDORED)


ANTLR_GRAMMARS := plsql postgresql tsql db2 cobol85




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
		$(TS_CC) $(filter-out -shared,$(TS_CFLAGS)) $${iflags} $${plf} -c "$${parser_c}" -o "$${tmpdir}/parser.o" 2>&1 || \
			{ echo "  ✗ $${name}: parser.c failed"; rm -rf "$${tmpdir}"; exit 1; }; \
		$(TS_CXX) $(filter-out -shared,$(TS_CXXFLAGS)) $${iflags} -c "$${scanner_cc}" -o "$${tmpdir}/scanner.o" 2>&1 || \
			{ echo "  ✗ $${name}: scanner.cc failed"; rm -rf "$${tmpdir}"; exit 1; }; \
		obj_files="$${tmpdir}/parser.o $${tmpdir}/scanner.o"; \
		if [ -n "$${extra_c}" ]; then \
			$(TS_CC) $(filter-out -shared,$(TS_CFLAGS)) $${iflags} -c "$${extra_c}" -o "$${tmpdir}/alloc.o" 2>&1 || \
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

# fetch_lbug_ext <server platform token>
#
# Puts the httpfs extension in the launcher payload, which is what makes a remote graph
# query offline: the core loads it by path from the extracted runtime and never calls
# INSTALL. NOTE: the server's platform tokens are its own and do NOT match GOOS/GOARCH —
# macOS is `osx` and Windows is `win`. curl -f is load-bearing: without it a 404 lands a
# 153-byte HTML error page in the payload and ships as if it were an extension.
define fetch_lbug_ext
	@mkdir -p $(LBUG_EXT_CACHE)/$(LBUG_EXT_VERSION)/$(1) cmd/launcher/runtime/lbug
	@if [ ! -s "$(LBUG_EXT_CACHE)/$(LBUG_EXT_VERSION)/$(1)/httpfs.lbug_extension" ]; then \
		echo "  → Downloading httpfs extension v$(LBUG_EXT_VERSION) for $(1)…"; \
		curl -fsSL "$(LBUG_EXT_HOST)/v$(LBUG_EXT_VERSION)/$(1)/httpfs/libhttpfs.lbug_extension" \
			-o "$(LBUG_EXT_CACHE)/$(LBUG_EXT_VERSION)/$(1)/httpfs.lbug_extension"; \
	fi
	cp -L "$(LBUG_EXT_CACHE)/$(LBUG_EXT_VERSION)/$(1)/httpfs.lbug_extension" cmd/launcher/runtime/lbug/httpfs.lbug_extension
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
	@echo "  Module grammars (bindings/go sources)"
	@echo "  ──────────────────────────────────────────────────────────────────"
	@for spec in $(GRAMMAR_MODULES); do \
		key=$$(echo "$$spec" | cut -d: -f1); \
		mod=$$(echo "$$spec" | cut -d: -f2); \
		sub=$$(echo "$$spec" | cut -d: -f3); \
		dir=$$(go list -m -f '{{.Dir}}' "$$mod" 2>/dev/null); \
		if [ -z "$$dir" ]; then echo "  ✗ $$key: module $$mod not resolved"; continue; fi; \
		if [ -n "$$sub" ]; then src="$$dir/$$sub/src"; else src="$$dir/src"; fi; \
		$(call compile_ts_grammar,$${key},$${src},$${src},,); \
	done
	@echo ""
	@echo "  Vendored grammars (internal/ast/treesitter)"
	@echo "  ──────────────────────────────────────────────────────────────────"
	@for key in $(GRAMMAR_VENDORED); do \
		src="internal/ast/treesitter/$${key}"; \
		$(call compile_ts_grammar,$${key},$${src},$${src},,); \
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


define bundle_ast
	@mkdir -p cmd/launcher/runtime/ast/queries
	cp internal/ast/queries/*.yaml cmd/launcher/runtime/ast/queries/
endef




# The go.mod written into node_modules is the fix for a long-standing local/CI divergence,
# and it is not a hack for its own sake: one of the UI's transitive packages (flatted) ships
# Go sources, so after `npm ci` the parent module's `./...` pattern starts matching a
# third-party package. `go list` skips any directory that declares its own module, so one
# stub file removes the whole tree from `./...` for EVERY go tool at once — vet, test,
# golangci-lint and govulncheck — instead of each of them carrying its own grep. It is
# regenerated here because npm owns the directory and wipes it.
#
# GO_PKGS_SKIP still names /node_modules/ as a belt-and-braces measure, for a checkout where
# npm ran before this target existed.
ui:
	cd internal/ui && npm ci --prefer-offline
	@printf 'module nodemodules\n\ngo 1.26\n' > internal/ui/node_modules/go.mod
	cd internal/ui && npm run build

ui-dev:
	cd internal/ui && npm run dev




setup-lbug:
	@go mod download github.com/LadybugDB/go-ladybug
	@chmod -R u+w "$(LBUG_MOD)" 2>/dev/null || true
	@mkdir -p $(LBUG_CACHE)
	@# go-ladybug v0.17.0 (cgo_bundled.go) expects a flat lib/ dir holding both
	@# the header (lbug.h) and the shared library: -I${SRCDIR}/lib / -L${SRCDIR}/lib.
	@# The release archives already bundle the header, so we extract them wholesale.
	@for plat in $(LBUG_PLATFORMS); do \
		case $$plat in \
		linux-amd64) \
			if [ ! -f "$(LBUG_MOD)/lib/liblbug.so" ] || [ ! -f "$(LBUG_MOD)/lib/lbug.h" ]; then \
				echo "  → Downloading liblbug for linux-x86_64…"; \
				mkdir -p "$(LBUG_MOD)/lib"; \
				curl -sSL "https://github.com/LadybugDB/ladybug/releases/latest/download/liblbug-linux-x86_64.tar.gz" -o "$(LBUG_CACHE)/liblbug-linux-x86_64.tar.gz"; \
				tar xzf "$(LBUG_CACHE)/liblbug-linux-x86_64.tar.gz" -C "$(LBUG_MOD)/lib"; \
			fi ;; \
		darwin) \
			if [ ! -f "$(LBUG_MOD)/lib/liblbug.dylib" ] || [ ! -f "$(LBUG_MOD)/lib/lbug.h" ]; then \
				echo "  → Downloading liblbug for darwin-arm64…"; \
				mkdir -p "$(LBUG_MOD)/lib"; \
				curl -sSL "https://github.com/LadybugDB/ladybug/releases/latest/download/liblbug-osx-arm64.tar.gz" -o "$(LBUG_CACHE)/liblbug-osx-arm64.tar.gz"; \
				tar xzf "$(LBUG_CACHE)/liblbug-osx-arm64.tar.gz" -C "$(LBUG_MOD)/lib"; \
			fi ;; \
		windows) \
			if [ ! -f "$(LBUG_MOD)/lib/lbug_shared.dll" ] || [ ! -f "$(LBUG_MOD)/lib/lbug.h" ]; then \
				echo "  → Downloading liblbug for windows-x86_64…"; \
				mkdir -p "$(LBUG_MOD)/lib"; \
				curl -sSL "https://github.com/LadybugDB/ladybug/releases/latest/download/liblbug-windows-x86_64.zip" -o "$(LBUG_CACHE)/liblbug-windows-x86_64.zip"; \
				unzip -qo "$(LBUG_CACHE)/liblbug-windows-x86_64.zip" -d "$(LBUG_MOD)/lib"; \
			fi ;; \
		*) echo "  ⚠ Unknown platform: $$plat" ;; \
		esac; \
	done

# curl_fetch <url> <destination>
#
# Downloads with retries and fails loudly on the real error.
#
# The plain `curl -sSL` this replaces had three problems: no retry, so one
# transient CDN timeout failed the whole build; no -f, so an HTTP error page
# was written to the file as if it were the payload; and no cleanup, so the
# following `mv` reported "cannot stat" and that misleading message is the one
# that ends up in the CI log, several lines below the actual cause.
#
# --retry-all-errors is what covers connection timeouts; --retry alone only
# retries responses curl considers transient.
define curl_fetch
	if ! curl -fSL --retry 5 --retry-delay 5 --retry-all-errors \
		--connect-timeout 30 --progress-bar "$(1)" -o "$(2).tmp"; then \
		rm -f "$(2).tmp"; \
		echo ""; \
		echo "  ✗ download failed: $(1)"; \
		echo "    the file was not written; nothing downstream will find it."; \
		exit 1; \
	fi; \
	mv "$(2).tmp" "$(2)"
endef

# fetch-lancedb builds the LanceDB native for THIS platform and puts it in the launcher
# payload. There is no cross-compile and no shared prebuilt: each platform builds its own,
# which is also what covers windows_amd64 and linux_arm64 — absent from the upstream
# release despite its release notes.
#
# The three edits below are the entire delta against LANCEDB_SHA: a crate-type line, the
# lancedb `aws` feature, and one lockfile bump. NO SOURCE IS PATCHED — all three are build
# configuration, and each is explained where it is pinned above.
fetch-lancedb:
	@command -v cargo >/dev/null 2>&1 || { \
		echo "✗ cargo not found — the LanceDB native is built from source."; \
		echo "  Install Rust $(LANCEDB_RUST) or newer: https://rustup.rs"; \
		echo "  Then re-run. Nothing else in the build needs Rust."; \
		exit 1; \
	}
	@mkdir -p $(LANCEDB_CACHE) $(LANCEDB_LIB_DIR)
	@if [ ! -d "$(LANCEDB_CACHE)/src/.git" ]; then \
		echo "→ Cloning lancedb-go…"; \
		git clone -q $(LANCEDB_REPO) "$(LANCEDB_CACHE)/src"; \
	fi
	@cd "$(LANCEDB_CACHE)/src" && \
		if [ "$$(git rev-parse HEAD)" != "$(LANCEDB_SHA)" ]; then \
			echo "→ Checking out $(LANCEDB_SHA)…"; \
			git fetch -q origin $(LANCEDB_SHA) 2>/dev/null || git fetch -q origin; \
			git checkout -q $(LANCEDB_SHA); \
			git checkout -q -- rust/Cargo.toml rust/Cargo.lock; \
		fi
	@cd "$(LANCEDB_CACHE)/src" && \
		grep -q 'crate-type = \["staticlib", "cdylib"\]' rust/Cargo.toml || { \
			echo "→ Re-enabling cdylib…"; \
			sed -i.bak 's/crate-type = \["staticlib"\]/crate-type = ["staticlib", "cdylib"]/' rust/Cargo.toml && \
			rm -f rust/Cargo.toml.bak; \
		}
	@cd "$(LANCEDB_CACHE)/src" && \
		grep -q 'features = \["aws"\]' rust/Cargo.toml || { \
			echo "→ Enabling the lancedb aws feature (s3:// support)…"; \
			sed -i.bak 's/tag = "v0.24.0", default-features = false }/tag = "v0.24.0", default-features = false, features = ["aws"] }/' rust/Cargo.toml && \
			rm -f rust/Cargo.toml.bak; \
		}
	@cd "$(LANCEDB_CACHE)/src/rust" && \
		grep -q 'version = "$(LANCEDB_ETHNUM)"' Cargo.lock || { \
			echo "→ Bumping ethnum to $(LANCEDB_ETHNUM)…"; \
			cargo update -p ethnum --precise $(LANCEDB_ETHNUM) >/dev/null; \
		}
	@cd "$(LANCEDB_CACHE)/src/rust" && \
		echo "→ Building the LanceDB native (this takes minutes on a cold cache)…" && \
		cargo build --release
	@set -e; \
	src="$(LANCEDB_CACHE)/src/rust/target/release"; \
	case "$$(go env GOOS)" in \
		darwin)  lib=liblancedb_go.dylib ;; \
		windows) lib=lancedb_go.dll ;; \
		*)       lib=liblancedb_go.so ;; \
	esac; \
	[ -s "$$src/$$lib" ] || { echo "✗ expected $$lib in $$src"; ls -la "$$src" | head; exit 1; }; \
	cp -L "$$src/$$lib" $(LANCEDB_LIB_DIR)/; \
	echo "  ✓ $$lib ($$(du -h $(LANCEDB_LIB_DIR)/$$lib | cut -f1)) → $(LANCEDB_LIB_DIR)/"; \
	printf '%s\n' '$(LANCEDB_SHA)' > "$(LANCEDB_LIB_DIR)/lancedb_go_build.sha"

# lancedb-native is the CHEAP guard: a file target, so an existing library is left alone and
# `make test` does not shell out to cargo on every run. `fetch-lancedb` stays as the explicit
# "rebuild it" entry point.
#
# When the library is missing, it resolves like every other native this tree links: from what the
# last install already extracted. The launcher payload carries liblancedb_go into
# $(LANCEDB_RUNTIME_SOURCE), the same directory embedding_local.go reads libonnxruntime from, so a
# checkout that has ANY working install can link and test without cargo. Only when neither the
# project copy nor the extracted runtime has it does this build from source. The runtime copy does
# not record which LANCEDB_SHA produced it; if the pin moved since that install, rebuild explicitly.
lancedb-native:
	@if [ -s "$(LANCEDB_LIB)" ]; then exit 0; fi; \
	runtime_lib="$(LANCEDB_RUNTIME_SOURCE)/$(LANCEDB_LIB_NAME)"; \
	if [ -s "$$runtime_lib" ]; then \
		mkdir -p "$(LANCEDB_LIB_DIR)"; \
		case "$(LANCEDB_GOOS)" in \
			windows) cp -L "$$runtime_lib" "$(LANCEDB_LIB)" ;; \
			*)       ln -sf "$$runtime_lib" "$(LANCEDB_LIB)" ;; \
		esac; \
		echo "  ✓ LanceDB native linked to $$runtime_lib (no cargo needed)"; \
		echo "    provenance unrecorded — if LANCEDB_SHA moved since that install, 'make fetch-lancedb' rebuilds"; \
		exit 0; \
	fi; \
	echo "  → LanceDB native missing ($(LANCEDB_LIB)) and none in $(LANCEDB_RUNTIME_SOURCE); building…"; \
	$(MAKE) --no-print-directory fetch-lancedb

# lancedb-cgo-env prints the CGO flags a build needs, so a developer can export them.
# The header and the library both live in the cache; nothing is copied into the repo.
lancedb-cgo-env:
	@echo "export CGO_CFLAGS=\"-I$(LANCEDB_CACHE)/src/include\""
	@echo "export CGO_LDFLAGS=\"-L$(LANCEDB_CACHE)/src/rust/target/release -llancedb_go\""

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




build: build-linux

# build-local builds for the HOST with `lancedb` linked in, which is the only way to get a
# binary whose search works end to end. The cross-compiled targets below cannot do this — see
# LOCAL_TAGS.
build-local: setup-lbug lancedb-native
	@mkdir -p $(BIN_DIR)
	CGO_ENABLED=1 go build -tags "$(LOCAL_TAGS)" -ldflags "$(LDFLAGS)" -o $(BIN_DIR)/$(BRAND)-local $(CMD)
	@echo "  ✓ $(BIN_DIR)/$(BRAND)-local"

install: build
	@mkdir -p $(PREFIX)
	@if [ -w "$(PREFIX)" ]; then \
		cp $(BIN_DIR)/$(BRAND)-linux-amd64 $(PREFIX)/$(BRAND); \
	else \
		sudo cp $(BIN_DIR)/$(BRAND)-linux-amd64 $(PREFIX)/$(BRAND); \
	fi
	@echo "  ✓ Installed to $(PREFIX)/$(BRAND)"
	@case ":$$PATH:" in \
		*":$(PREFIX):"*) ;; \
		*) echo "  ⚠ $(PREFIX) is not in your PATH. Add it: export PATH=\"\$$PATH:$(PREFIX)\"" ;; \
	esac

install-darwin: build-darwin
	@mkdir -p $(PREFIX)
	@if [ -w "$(PREFIX)" ]; then \
		cp $(BIN_DIR)/$(BRAND)-darwin-arm64 $(PREFIX)/$(BRAND); \
	else \
		sudo cp $(BIN_DIR)/$(BRAND)-darwin-arm64 $(PREFIX)/$(BRAND); \
	fi
	@echo "  ✓ Installed to $(PREFIX)/$(BRAND)"
	@xattr -d com.apple.quarantine $(PREFIX)/$(BRAND) 2>/dev/null || true
	@codesign --sign - --force $(PREFIX)/$(BRAND) 2>/dev/null || true
	@case ":$$PATH:" in \
		*":$(PREFIX):"*) ;; \
		*) echo "  ⚠ $(PREFIX) is not in your PATH. Add it: export PATH=\"\$$PATH:$(PREFIX)\"" ;; \
	esac

install-windows: build-windows-native
	@# Default: %PROGRAMFILES%\graphit  (global — equivalent of /usr/local/bin, may need admin)
	@# Override: make install-windows PREFIX_WIN='C:\Tools\graphit'
	@_dir="$(PREFIX_WIN)"; \
	if [ -z "$$_dir" ]; then _dir="$${PROGRAMFILES}/graphit"; fi; \
	_dir="$$(echo "$$_dir" | sed 's|\\\\|/|g')"; \
	mkdir -p "$$_dir" || { echo "  ✗ Cannot create $$_dir. Try running as Administrator."; exit 1; }; \
	cp .build/$(BRAND)-windows-amd64.exe "$$_dir/$(BRAND).exe"; \
	_windir="$$(echo "$$_dir" | sed 's|/|\\\\|g')"; \
	echo "  ✓ Installed to $$_windir\\$(BRAND).exe"; \
	powershell.exe -NoProfile -ExecutionPolicy Bypass -Command \
		"\$$d = '$$_windir'; \
		\$$syspath = [System.Environment]::GetEnvironmentVariable('PATH', 'Machine'); \
		if (\$$syspath -notlike \"*\$$d*\") { \
			try { \
				[System.Environment]::SetEnvironmentVariable('PATH', \"\$$syspath;\$$d\", 'Machine'); \
				Write-Host '  ✓ Added to system PATH (restart terminal to take effect)' -ForegroundColor Green \
			} catch { \
				\$$up = [System.Environment]::GetEnvironmentVariable('PATH', 'User'); \
				[System.Environment]::SetEnvironmentVariable('PATH', \"\$$up;\$$d\", 'User'); \
				Write-Host '  ⚠ No admin rights — added to user PATH instead (restart terminal)' -ForegroundColor Yellow \
			} \
		} else { Write-Host \"  ✓ \$$d already in PATH\" -ForegroundColor Green }" 2>/dev/null || \
	echo "  ⚠ Could not update PATH automatically. Add manually: $$_windir"


build-linux: ui setup-lbug fetch-ort-linux lancedb-native
	@mkdir -p cmd/launcher/runtime
	GOOS=linux GOARCH=amd64 CGO_ENABLED=1 go build -tags "$(BUILD_TAGS)" -ldflags "$(LDFLAGS) $(STRIP_LDFLAGS)" -o cmd/launcher/runtime/$(BRAND)-core $(CMD)
	GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp ./cmd/mcp
	find $$(go env GOPATH)/pkg/mod/github.com/!ladybug!d!b/go-ladybug@$(LBUG_VERSION)/lib -maxdepth 1 -name "liblbug.so" -exec cp -L {} cmd/launcher/runtime/ \;
	cd cmd/launcher/runtime && cp liblbug.so liblbug.so.0
	cp -L $(ORT_CACHE)/onnxruntime-linux-x64-$(ORT_VERSION)/lib/libonnxruntime.so cmd/launcher/runtime/
	@# The search engine. It travels beside the binary, which is what the $ORIGIN rpath in
	@# internal/lancestore/cgo_lancedb.go resolves against — see that file for why there are two.
	cp -L $(LANCEDB_LIB) cmd/launcher/runtime/
	$(call fetch_lbug_ext,linux_amd64)
	$(call bundle_ast)
	@mkdir -p $(BIN_DIR)
	GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS)" -o $(BIN_DIR)/$(BRAND)-linux-amd64 ./cmd/launcher
	rm -rf cmd/launcher/runtime/*

# ICU is NOT bundled, on any platform.
#
# Nothing in the bundle loads it. Measured on linux: `objdump -p` shows no ICU in the
# NEEDED list of graphit-core, liblbug.so or libonnxruntime.so; `strings -a` finds no
# "libicu" in any of them, so it is not dlopen'd by name either; and a runtime with the
# ICU files deleted still answers `ast query --hybrid`, which exercises LadybugDB, the
# full-text index, the vector index and ONNX inference at once. LadybugDB's own build
# documentation lists no ICU dependency on any platform.
#
# It cost 37-73 MiB on linux, varying with how many ICU majors the build machine had,
# because the glob took every one of them.
#
# macOS and Windows were NOT verified — that needs an artifact of each platform, and the
# Engineer chose to remove it anyway and put it back if something turns out to need it.
# The failure mode differs by platform and is worth knowing: on macOS a missing dylib
# aborts the process at startup, so a break there is total rather than partial.
#
# libicu-dev / icu4c may still be needed to COMPILE. That is separate, and unchanged.
build-darwin: ui setup-lbug fetch-ort-darwin lancedb-native
	@mkdir -p cmd/launcher/runtime
	GOOS=darwin GOARCH=arm64 CGO_ENABLED=1 go build -tags "$(BUILD_TAGS)" -ldflags "$(LDFLAGS) $(STRIP_LDFLAGS)" -o cmd/launcher/runtime/$(BRAND)-core $(CMD)
	GOOS=darwin GOARCH=arm64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp ./cmd/mcp
	find $$(go env GOPATH)/pkg/mod/github.com/!ladybug!d!b/go-ladybug@$(LBUG_VERSION)/lib -maxdepth 1 -name "liblbug.dylib" -exec cp -L {} cmd/launcher/runtime/ \;
	cd cmd/launcher/runtime && cp liblbug.dylib liblbug.0.dylib
	cp -L $(ORT_CACHE)/onnxruntime-osx-arm64-$(ORT_VERSION)/lib/libonnxruntime.dylib cmd/launcher/runtime/
	cp -L $(LANCEDB_LIB) cmd/launcher/runtime/
	$(call fetch_lbug_ext,osx_arm64)
	$(call bundle_ast)
	@mkdir -p $(BIN_DIR)
	GOOS=darwin GOARCH=arm64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS)" -o $(BIN_DIR)/$(BRAND)-darwin-arm64 ./cmd/launcher
	rm -rf cmd/launcher/runtime/*

# Windows releases are built natively under MSYS2. Do not replace this with a
# cross-build: LanceDB is compiled for the host and search must remain available.
build-windows-native: ui setup-lbug fetch-ort-windows lancedb-native
	@mkdir -p cmd/launcher/runtime
	CGO_ENABLED=1 CGO_CFLAGS="-I/mingw64/include" CGO_CXXFLAGS="-I/mingw64/include" CGO_LDFLAGS="-lstdc++" go build -tags "$(BUILD_TAGS)" -ldflags "$(LDFLAGS) $(STRIP_LDFLAGS)" -o cmd/launcher/runtime/$(BRAND)-core.exe $(CMD)
	CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp.exe ./cmd/mcp
	cp -L $(LANCEDB_LIB) cmd/launcher/runtime/
	GOPATH_UNIX=$$(cygpath -u "$$(go env GOPATH)") && find $$GOPATH_UNIX/pkg/mod/github.com/!ladybug!d!b/go-ladybug@$(LBUG_VERSION)/lib -maxdepth 1 -name "lbug_shared.dll" -exec cp -L {} cmd/launcher/runtime/ \;
	cp /mingw64/bin/libgcc_s_seh-1.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp /mingw64/bin/libstdc++-6.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp /mingw64/bin/libwinpthread-1.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp -L $(ORT_CACHE)/onnxruntime-win-x64-$(ORT_VERSION)/lib/onnxruntime.dll cmd/launcher/runtime/
	$(call fetch_lbug_ext,win_amd64)
	$(call bundle_ast)
	@mkdir -p $(BIN_DIR)
	CGO_ENABLED=0 go build -ldflags "$(LDFLAGS)" -o $(BIN_DIR)/$(BRAND)-windows-amd64.exe ./cmd/launcher
	rm -rf cmd/launcher/runtime/*

# EVERY BUILD HAPPENS ON ITS OWN PLATFORM NOW, so there is nothing for one machine to build all of.
#
build-all:
	@echo "  ✗ build-all no longer exists as one machine's job."
	@echo ""
	@echo "    The LanceDB native is built from Rust source for the host and cannot be"
	@echo "    cross-compiled, so a binary for another platform cannot be produced here — and one"
	@echo "    built without it has no search at all, which is worse than not building it."
	@echo ""
	@echo "    Release runs three jobs, one per runner: .github/workflows/release.yml"
	@echo "    For this machine:  make build-local"
	@exit 1




# The rm -rf below sweeps the isolated test homes from the PREVIOUS run, not this one.
# internal/brand's init points HOME at a fresh directory under that parent for every test
# binary (see internal/brand/testhome.go for why), and nothing can delete them at exit:
# os.Exit skips deferred functions, and a package without its own TestMain has no hook
# after m.Run(). Sweeping before the run bounds the residue to one run's worth, and does
# it without yanking HOME out from under a process a leaked test left running — which
# sweeping afterwards would do.
test: setup-lbug lancedb-native $(ORT_HOST_FETCH)
	@LBUG_LIB="$(LBUG_MOD)/lib"; \
	if [ -f "$$LBUG_LIB/liblbug.so" ] && [ ! -f "$$LBUG_LIB/liblbug.so.0" ]; then \
		cp -L "$$LBUG_LIB/liblbug.so" "$$LBUG_LIB/liblbug.so.0"; \
	fi; \
	rm -rf "$${TMPDIR:-/tmp}/$(BRAND)-test-homes"; \
	status=0; \
	echo "  → Running tests with race detector (project code)…"; \
	LD_LIBRARY_PATH="$$LBUG_LIB:$(ORT_HOST_LIB):$$LD_LIBRARY_PATH" \
	DYLD_LIBRARY_PATH="$(ORT_HOST_LIB):$$DYLD_LIBRARY_PATH" \
	$(BRAND_ENV)_MODEL_CACHE="$(MODEL_CACHE)" go test -race -tags "$(LOCAL_TAGS)" -coverprofile=coverage.out -covermode=atomic -p $(GO_TEST_P) \
		$$(go list ./... | grep -Ev "$(GO_PKGS_SKIP)") || status=1; \
	echo "  → Running tests without race detector (generated parsers, appended)…"; \
	LD_LIBRARY_PATH="$$LBUG_LIB:$(ORT_HOST_LIB):$$LD_LIBRARY_PATH" \
	DYLD_LIBRARY_PATH="$(ORT_HOST_LIB):$$DYLD_LIBRARY_PATH" \
	$(BRAND_ENV)_MODEL_CACHE="$(MODEL_CACHE)" go test -tags "$(LOCAL_TAGS)" -coverprofile=coverage-parsers.out -covermode=atomic -p $(GO_TEST_P) \
		$$(go list ./... | grep -E "$(GO_PKGS_PARSERS)" | grep -v "/node_modules/") || status=1; \
	if [ -f coverage-parsers.out ]; then \
		tail -n +2 coverage-parsers.out >> coverage.out; \
		rm -f coverage-parsers.out; \
	fi; \
	exit $$status

test-short: setup-lbug lancedb-native $(ORT_HOST_FETCH)
	@LBUG_LIB="$(LBUG_MOD)/lib"; \
	if [ -f "$$LBUG_LIB/liblbug.so" ] && [ ! -f "$$LBUG_LIB/liblbug.so.0" ]; then \
		cp -L "$$LBUG_LIB/liblbug.so" "$$LBUG_LIB/liblbug.so.0"; \
	fi; \
	rm -rf "$${TMPDIR:-/tmp}/$(BRAND)-test-homes"; \
	status=0; \
	echo "  → Running tests with race detector (-short, skips heavy model/LanceDB)…"; \
	LD_LIBRARY_PATH="$$LBUG_LIB:$(ORT_HOST_LIB):$$LD_LIBRARY_PATH" \
	DYLD_LIBRARY_PATH="$(ORT_HOST_LIB):$$DYLD_LIBRARY_PATH" \
	$(BRAND_ENV)_MODEL_CACHE="$(MODEL_CACHE)" go test -short -race -tags "$(LOCAL_TAGS)" -coverprofile=coverage.out -covermode=atomic -p $(GO_TEST_P) \
		$$(go list ./... | grep -Ev "$(GO_PKGS_SKIP)") || status=1; \
	echo "  → Running tests without race detector (generated parsers, -short)…"; \
	LD_LIBRARY_PATH="$$LBUG_LIB:$(ORT_HOST_LIB):$$LD_LIBRARY_PATH" \
	DYLD_LIBRARY_PATH="$(ORT_HOST_LIB):$$DYLD_LIBRARY_PATH" \
	$(BRAND_ENV)_MODEL_CACHE="$(MODEL_CACHE)" go test -short -tags "$(LOCAL_TAGS)" -coverprofile=coverage-parsers.out -covermode=atomic -p $(GO_TEST_P) \
		$$(go list ./... | grep -E "$(GO_PKGS_PARSERS)" | grep -v "/node_modules/") || status=1; \
	if [ -f coverage-parsers.out ]; then \
		tail -n +2 coverage-parsers.out >> coverage.out; \
		rm -f coverage-parsers.out; \
	fi; \
	exit $$status

# No --build-tags here on purpose. The tags live in .golangci.yml, which is the one file both
# this target and the GitHub lint job read; passing the flag here OVERRODE that list and is
# exactly how local lint (lancedb) and CI lint (the stale fts5 from the config) came to
# analyse two different builds. See the comment on `run.build-tags` in .golangci.yml.
lint: lancedb-native
	golangci-lint run ./...

# govulncheck loads and type-checks the packages before it looks at anything, so it
# needs BUILD_TAGS for the same reason vet and lint do. Without the tag, internal/ast
# and internal/wiki resolved to a `!fts5` guard file instead of the package, and the
# load aborts on the undefined guard symbol before a single vulnerability is reported —
# which reads like a broken tool rather than a missing flag.
#
# The version is PINNED, not @latest. `@latest` resolves over the network on every run — so
# the check is neither reproducible nor cacheable, and a new govulncheck release turns a green
# pipeline red without a commit. Pinned, the binary is built once and then served from the
# build cache; the vulnerability DATABASE is still fetched live, which is the part that has to
# be current. Bump GOVULNCHECK_VERSION deliberately.
#
# lancedb-native is a prerequisite for the same reason it is one for vet, lint and test: the
# tag is not optional, and every tool that type-checks with it needs the native resolvable.
vulncheck: lancedb-native
	go run golang.org/x/vuln/cmd/govulncheck@$(GOVULNCHECK_VERSION) -tags "$(LOCAL_TAGS)" ./...

# actionlint validates the GitHub workflow files, and it is in `ci` because of a defect it
# would have caught the day it was introduced: `permissions: code-quality: write` is not a
# GitHub permission scope, GitHub rejects a workflow file containing an unknown one, and a
# rejected file does not fail — it never runs. The repository showed no CI runs at all, which
# looks like "nothing to report" rather than like six missing jobs.
#
# Cheap enough to be unconditional: one pinned binary, milliseconds per file.
actionlint:
	@go run github.com/rhysd/actionlint/cmd/actionlint@$(ACTIONLINT_VERSION) -no-color .github/workflows/*.yml

ui-lint:
	cd internal/ui && npm run lint

fmt:
	gofmt -w .

# GO_PKGS_SKIP keeps the generated grammars off the list, but that is not enough:
# internal/ast and the sidecar IMPORT them, and go vet reports the diagnostics of a
# dependency whenever it has to analyse that dependency itself — which it does only
# when the analysis cache is cold. So this passed locally and failed in CI, on
# 255k lines of generated db2_parser.go where ANTLR emits a return after a panic.
#
# `unreachable` is the only analyser that fires on the generated code, and there is
# no way to ask vet for "diagnostics of the named packages only", so it is the one
# turned off. Everything else vet does still runs over the whole project.
#
# BUILD_TAGS is passed for the same reason lint sets build-tags in .golangci.yml: vet
# has to analyse the configuration that actually ships. Without the tag it analyses a
# build where internal/ast and internal/wiki were a `!fts5` guard file instead of the
# package — so it would both miss every real diagnostic in them and stop on the guard.
vet: lancedb-native
	go vet -tags "$(LOCAL_TAGS)" -unreachable=false $$(go list -tags "$(LOCAL_TAGS)" ./... | grep -Ev "$(GO_PKGS_SKIP)")

# ci-fast is the PR gate: static checks in parallel and tests with -short (skips model download and heavy LanceDB paths).
ci-fast: lancedb-native
	@echo "  → Running actionlint, vet, lint, ui-lint in parallel (vulncheck and full test are ci)…"
	@$(MAKE) -j4 actionlint vet lint ui-lint
	@$(MAKE) test-short

# `ui` runs first and alone because vet, lint and test all need internal/ui/dist to exist —
# it is embedded — and because it is what creates internal/ui/node_modules, whose go.mod stub
# has to be in place before any go tool expands ./...
ci: lancedb-native
	@echo "  → Building UI, then actionlint/vet/lint/vulncheck/ui-lint in parallel, then full test…"
	@$(MAKE) ui
	@$(MAKE) -j5 actionlint vet lint vulncheck ui-lint
	@$(MAKE) test
	@echo ""
	@echo "  ✅ All CI checks passed."
	@echo ""


check: actionlint vet lint vulncheck test
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
