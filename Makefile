.PHONY: build build-all build-local install install-darwin install-windows clean fmt vet run ui ui-dev setup-lbug fetch-lancedb lancedb-native build-local lancedb-cgo-env \
       fetch-ort-linux fetch-ort-darwin fetch-ort-windows lint \
	   ui-lint ci ci-fast check test test-short test-race build-windows-native \
       grammars grammars-treesitter grammars-antlr grammars-clean

MODULE   := github.com/graphit-labs/graphit-code
CMD      := ./cmd/graphit
BIN_DIR  := .build

BRAND        ?= graphit
BRAND_ENV    := $(shell echo $(BRAND) | tr '[:lower:]' '[:upper:]')
DISPLAY_NAME ?= Graphit Code: A Powerful Agent Harness for Enterprise Software Ecosystems
VERSION      ?= dev
GITHUB_REPO  ?= graphit-labs/graphit-code
DEFAULT_HUB_BUCKET   ?=
DEFAULT_HUB_REGION   ?=
DEFAULT_HUB_ENDPOINT ?=
SELF_UPDATE_URL ?=
COMPILE_CONFIG ?=
PREFIX       ?= /usr/local/bin
PREFIX_WIN   ?=

ifeq ($(origin BUILD_ID), undefined)
  ifeq ($(OS),Windows_NT)
    BUILD_ID := $(shell powershell -Command "[System.Guid]::NewGuid().ToString()")
  else
    BUILD_ID := $(shell cat /proc/sys/kernel/random/uuid 2>/dev/null || uuidgen)
  endif
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

BUILD_TAGS := lancedb

LOCAL_TAGS := $(BUILD_TAGS)


LANCEDB_LIB_DIR := .native

LANCEDB_GOOS := $(shell go env GOOS)
ifeq ($(LANCEDB_GOOS),darwin)
LANCEDB_LIB_NAME := liblancedb_go.dylib
else ifeq ($(LANCEDB_GOOS),windows)
LANCEDB_LIB_NAME := lancedb_go.dll
else
LANCEDB_LIB_NAME := liblancedb_go.so
endif
LANCEDB_LIB := $(LANCEDB_LIB_DIR)/$(LANCEDB_LIB_NAME)

GO_PKGS_SKIP    := /antlr/|/treesitter/|/node_modules/
GO_PKGS_PARSERS := /antlr/|/treesitter/

GOVULNCHECK_VERSION := v1.7.0
ACTIONLINT_VERSION  := v1.7.7

GO_TEST_P           ?= 1
GO_TEST_PARALLEL    ?= 2
GO_TEST_GOMAXPROCS  ?= 2
GO_TEST_TIMEOUT     ?= 15m

ORT_VERSION  := 1.26.0
ORT_CACHE    := /tmp/onnxruntime-cache

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

STRIP_LDFLAGS ?= -s -w


LANCEDB_SHA   := fa14ce29c7724354f2cea630a1d3488b56bbd64b
LANCEDB_REPO  := https://github.com/lancedb/lancedb-go.git
LANCEDB_CACHE ?= /tmp/lancedb-native-cache
LANCEDB_RUNTIME_SOURCE ?= $(if $($(BRAND_ENV)_GLOBAL_DIR),$($(BRAND_ENV)_GLOBAL_DIR),$(HOME)/.$(BRAND))/runtime/dev

LANCEDB_RUST     := 1.98.0
LANCEDB_ETHNUM   := 1.5.3


LBUG_VERSION := v0.17.0

LBUG_EXT_VERSION := 0.18.1
LBUG_EXT_HOST    := https://extension.ladybugdb.com
LBUG_EXT_CACHE   := /tmp/lbug-extension-cache
LBUG_MOD     := $(shell go env GOPATH)/pkg/mod/github.com/!ladybug!d!b/go-ladybug@$(LBUG_VERSION)
LBUG_CACHE   := /tmp/lbug-cache

LBUG_PLATFORMS ?= $(shell uname -s | sed 's/Darwin/darwin/;s/Linux/linux-amd64/;s/MINGW.*/windows/')



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




test: setup-lbug lancedb-native $(ORT_HOST_FETCH)
	@LBUG_LIB="$(LBUG_MOD)/lib"; \
	TEST_HOME_ROOT="$${TMPDIR:-/tmp}/$(BRAND)-test-homes"; \
	if [ -f "$$LBUG_LIB/liblbug.so" ] && [ ! -f "$$LBUG_LIB/liblbug.so.0" ]; then \
		cp -L "$$LBUG_LIB/liblbug.so" "$$LBUG_LIB/liblbug.so.0"; \
	fi; \
	rm -rf "$$TEST_HOME_ROOT"; \
	status=0; \
	echo "  → Running the bounded test suite…"; \
	LD_LIBRARY_PATH="$$LBUG_LIB:$(ORT_HOST_LIB):$$LD_LIBRARY_PATH" \
	DYLD_LIBRARY_PATH="$(ORT_HOST_LIB):$$DYLD_LIBRARY_PATH" \
	GRAPHIT_TEST_HOME_ROOT="$$TEST_HOME_ROOT" GOMAXPROCS="$(GO_TEST_GOMAXPROCS)" \
	go test -count=1 -tags "$(LOCAL_TAGS)" -coverprofile=coverage.out -covermode=atomic \
		-p $(GO_TEST_P) -parallel $(GO_TEST_PARALLEL) -timeout $(GO_TEST_TIMEOUT) \
		$$(go list ./... | grep -Ev "$(GO_PKGS_SKIP)") || status=1; \
	echo "  → Running generated parser tests…"; \
	LD_LIBRARY_PATH="$$LBUG_LIB:$(ORT_HOST_LIB):$$LD_LIBRARY_PATH" \
	DYLD_LIBRARY_PATH="$(ORT_HOST_LIB):$$DYLD_LIBRARY_PATH" \
	GRAPHIT_TEST_HOME_ROOT="$$TEST_HOME_ROOT" GOMAXPROCS="$(GO_TEST_GOMAXPROCS)" \
	go test -count=1 -tags "$(LOCAL_TAGS)" \
		-p $(GO_TEST_P) -parallel $(GO_TEST_PARALLEL) -timeout $(GO_TEST_TIMEOUT) \
		$$(go list -f '{{if .TestGoFiles}}{{.ImportPath}}{{end}}' ./internal/ast/antlr/... | sed '/^$$/d') || status=1; \
	rm -rf "$$TEST_HOME_ROOT"; \
	exit $$status

test-short: setup-lbug lancedb-native $(ORT_HOST_FETCH)
	@LBUG_LIB="$(LBUG_MOD)/lib"; \
	TEST_HOME_ROOT="$${TMPDIR:-/tmp}/$(BRAND)-test-homes"; \
	if [ -f "$$LBUG_LIB/liblbug.so" ] && [ ! -f "$$LBUG_LIB/liblbug.so.0" ]; then \
		cp -L "$$LBUG_LIB/liblbug.so" "$$LBUG_LIB/liblbug.so.0"; \
	fi; \
	rm -rf "$$TEST_HOME_ROOT"; \
	status=0; \
	echo "  → Running the bounded short test suite…"; \
	LD_LIBRARY_PATH="$$LBUG_LIB:$(ORT_HOST_LIB):$$LD_LIBRARY_PATH" \
	DYLD_LIBRARY_PATH="$(ORT_HOST_LIB):$$DYLD_LIBRARY_PATH" \
	GRAPHIT_TEST_HOME_ROOT="$$TEST_HOME_ROOT" GOMAXPROCS="$(GO_TEST_GOMAXPROCS)" \
	go test -short -count=1 -tags "$(LOCAL_TAGS)" -coverprofile=coverage.out -covermode=atomic \
		-p $(GO_TEST_P) -parallel $(GO_TEST_PARALLEL) -timeout $(GO_TEST_TIMEOUT) \
		$$(go list ./... | grep -Ev "$(GO_PKGS_SKIP)") || status=1; \
	echo "  → Running generated parser tests (-short)…"; \
	LD_LIBRARY_PATH="$$LBUG_LIB:$(ORT_HOST_LIB):$$LD_LIBRARY_PATH" \
	DYLD_LIBRARY_PATH="$(ORT_HOST_LIB):$$DYLD_LIBRARY_PATH" \
	GRAPHIT_TEST_HOME_ROOT="$$TEST_HOME_ROOT" GOMAXPROCS="$(GO_TEST_GOMAXPROCS)" \
	go test -short -count=1 -tags "$(LOCAL_TAGS)" \
		-p $(GO_TEST_P) -parallel $(GO_TEST_PARALLEL) -timeout $(GO_TEST_TIMEOUT) \
		$$(go list -f '{{if .TestGoFiles}}{{.ImportPath}}{{end}}' ./internal/ast/antlr/... | sed '/^$$/d') || status=1; \
	rm -rf "$$TEST_HOME_ROOT"; \
	exit $$status

test-race: setup-lbug lancedb-native $(ORT_HOST_FETCH)
	@LBUG_LIB="$(LBUG_MOD)/lib"; \
	TEST_HOME_ROOT="$${TMPDIR:-/tmp}/$(BRAND)-test-homes"; \
	rm -rf "$$TEST_HOME_ROOT"; \
	LD_LIBRARY_PATH="$$LBUG_LIB:$(ORT_HOST_LIB):$$LD_LIBRARY_PATH" \
	DYLD_LIBRARY_PATH="$(ORT_HOST_LIB):$$DYLD_LIBRARY_PATH" \
	GRAPHIT_TEST_HOME_ROOT="$$TEST_HOME_ROOT" GOMAXPROCS="$(GO_TEST_GOMAXPROCS)" \
	go test -race -count=1 -tags "$(LOCAL_TAGS)" -p 1 -parallel $(GO_TEST_PARALLEL) \
		-timeout $(GO_TEST_TIMEOUT) \
		./internal/fswatch/... ./internal/livesearch/... ./internal/mcpproxy/... \
		./internal/projectlock/... ./internal/sessionhook/... ./internal/sysutil/... ./internal/task/...; \
	status=$$?; \
	rm -rf "$$TEST_HOME_ROOT"; \
	exit $$status

lint: lancedb-native
	golangci-lint run ./...

vulncheck: lancedb-native
	go run golang.org/x/vuln/cmd/govulncheck@$(GOVULNCHECK_VERSION) -tags "$(LOCAL_TAGS)" ./...

actionlint:
	@go run github.com/rhysd/actionlint/cmd/actionlint@$(ACTIONLINT_VERSION) -no-color .github/workflows/*.yml

ui-lint:
	cd internal/ui && npm run lint

fmt:
	gofmt -w .

vet: lancedb-native
	go vet -tags "$(LOCAL_TAGS)" -unreachable=false $$(go list -tags "$(LOCAL_TAGS)" ./... | grep -Ev "$(GO_PKGS_SKIP)")

ci-fast: lancedb-native
	@echo "  → Running bounded static checks, then the short suite…"
	@$(MAKE) actionlint
	@$(MAKE) vet
	@$(MAKE) lint
	@$(MAKE) ui-lint
	@$(MAKE) test-short

ci: lancedb-native
	@echo "  → Building UI, then static checks, the bounded full suite, and focused race checks…"
	@$(MAKE) ui
	@$(MAKE) actionlint
	@$(MAKE) vet
	@$(MAKE) lint
	@$(MAKE) vulncheck
	@$(MAKE) ui-lint
	@$(MAKE) test
	@$(MAKE) test-race
	@echo ""
	@echo "  ✅ All CI checks passed."
	@echo ""


check: actionlint vet lint vulncheck test test-race
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
