.PHONY: build build-all install install-darwin install-windows clean fmt vet run ui ui-dev setup-lbug \
       fetch-ort-linux fetch-ort-darwin fetch-ort-windows fetch-model lint \
       ui-lint ci check test build-windows-native \
       grammars grammars-treesitter grammars-antlr grammars-clean

MODULE   := github.com/graphit-labs/graphit-code
CMD      := ./cmd/graphit
BIN_DIR  := .build

BRAND        ?= graphit
DISPLAY_NAME ?= Graphit Code: A Powerful Agent Harness for Enterprise Software Ecosystems
VERSION      ?= v0.1.27
GITHUB_REPO  ?= graphit-labs/graphit-code
DEFAULT_HUB_REPO    ?=
DEFAULT_MEMORY_REPO ?=
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
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.DefaultHubRepoURL=$(DEFAULT_HUB_REPO)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.DefaultMemoryRepoURL=$(DEFAULT_MEMORY_REPO)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/brand.SelfUpdateURL=$(SELF_UPDATE_URL)'
LDFLAGS += -X 'github.com/graphit-labs/graphit-code/internal/config.CompiledDefaults=$(COMPILE_CONFIG)'

BUILD_TAGS := fts5

# Must satisfy the ORT_API_VERSION the onnxruntime_go binding in go.mod compiles
# against (v1.31.0 declares 26). A runtime older than that aborts at
# InitializeEnvironment with "requested API version [26] is not available", which
# leaves the embedder nil and degrades semantic search to FTS-only in silence.
# Bump this together with the binding.
ORT_VERSION  := 1.26.0
ORT_CACHE    := /tmp/onnxruntime-cache

MODEL_REPO   := mrsladoje/CodeRankEmbed-onnx-int8
MODEL_CACHE  := /tmp/coderankembed-cache

LBUG_VERSION := v0.17.0
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
    objc proto r sql svelte swift dart


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




ui:
	cd internal/ui && npm ci --prefer-offline
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




build: build-linux

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


build-linux: ui setup-lbug fetch-ort-linux fetch-model
	@mkdir -p cmd/launcher/runtime
	GOOS=linux GOARCH=amd64 CGO_ENABLED=1 go build -tags "$(BUILD_TAGS)" -ldflags "$(LDFLAGS)" -o cmd/launcher/runtime/$(BRAND)-core $(CMD)
	GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp ./cmd/mcp
	find $$(go env GOPATH)/pkg/mod/github.com/!ladybug!d!b/go-ladybug@$(LBUG_VERSION)/lib -maxdepth 1 -name "liblbug.so" -exec cp -L {} cmd/launcher/runtime/ \;
	cd cmd/launcher/runtime && cp liblbug.so liblbug.so.0
	find /usr/lib /lib -name "libicu*.so.[0-9]*" -exec cp -L {} cmd/launcher/runtime/ \; 2>/dev/null || true
	rm -f cmd/launcher/runtime/*.so.*.*
	cp -L $(ORT_CACHE)/onnxruntime-linux-x64-$(ORT_VERSION)/lib/libonnxruntime.so cmd/launcher/runtime/
	$(call bundle_model)
	$(call bundle_ast)
	@mkdir -p $(BIN_DIR)
	GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS)" -o $(BIN_DIR)/$(BRAND)-linux-amd64 ./cmd/launcher
	rm -rf cmd/launcher/runtime/*

build-darwin: ui setup-lbug fetch-ort-darwin fetch-model
	@mkdir -p cmd/launcher/runtime
	GOOS=darwin GOARCH=arm64 CGO_ENABLED=1 go build -tags "$(BUILD_TAGS)" -ldflags "$(LDFLAGS)" -o cmd/launcher/runtime/$(BRAND)-core $(CMD)
	GOOS=darwin GOARCH=arm64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp ./cmd/mcp
	find $$(go env GOPATH)/pkg/mod/github.com/!ladybug!d!b/go-ladybug@$(LBUG_VERSION)/lib -maxdepth 1 -name "liblbug.dylib" -exec cp -L {} cmd/launcher/runtime/ \;
	cd cmd/launcher/runtime && cp liblbug.dylib liblbug.0.dylib
	find /opt/homebrew/opt/icu4c/lib /usr/local/opt/icu4c/lib -name "libicu*.[0-9]*.dylib" -exec cp -L {} cmd/launcher/runtime/ \; 2>/dev/null || true
	rm -f cmd/launcher/runtime/*.*.*.dylib
	cp -L $(ORT_CACHE)/onnxruntime-osx-arm64-$(ORT_VERSION)/lib/libonnxruntime.dylib cmd/launcher/runtime/
	$(call bundle_model)
	$(call bundle_ast)
	@mkdir -p $(BIN_DIR)
	GOOS=darwin GOARCH=arm64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS)" -o $(BIN_DIR)/$(BRAND)-darwin-arm64 ./cmd/launcher
	rm -rf cmd/launcher/runtime/*

build-windows: ui setup-lbug fetch-ort-windows fetch-model
	@mkdir -p cmd/launcher/runtime
	CC=x86_64-w64-mingw32-gcc CXX=x86_64-w64-mingw32-g++ CGO_CFLAGS="-I/usr/x86_64-w64-mingw32/icu/include -I/usr/include" CGO_CXXFLAGS="-I/usr/x86_64-w64-mingw32/icu/include -I/usr/include" CGO_LDFLAGS="-L/usr/x86_64-w64-mingw32/icu/lib -licuuc -licuin -licudt -lstdc++ -static-libgcc -static-libstdc++" GOOS=windows GOARCH=amd64 CGO_ENABLED=1 go build -tags "$(BUILD_TAGS)" -ldflags "$(LDFLAGS)" -o cmd/launcher/runtime/$(BRAND)-core.exe $(CMD)
	GOOS=windows GOARCH=amd64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp.exe ./cmd/mcp
	find $$(go env GOPATH)/pkg/mod/github.com/!ladybug!d!b/go-ladybug@$(LBUG_VERSION)/lib -maxdepth 1 -name "lbug_shared.dll" -exec cp -L {} cmd/launcher/runtime/ \;
	find /usr/x86_64-w64-mingw32 -name "*.dll" -exec cp -L {} cmd/launcher/runtime/ \; 2>/dev/null || true
	cp -L $(ORT_CACHE)/onnxruntime-win-x64-$(ORT_VERSION)/lib/onnxruntime.dll cmd/launcher/runtime/
	$(call bundle_model)
	$(call bundle_ast)
	@mkdir -p $(BIN_DIR)
	GOOS=windows GOARCH=amd64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS)" -o $(BIN_DIR)/$(BRAND)-windows-amd64.exe ./cmd/launcher
	rm -rf cmd/launcher/runtime/*

build-windows-native: ui setup-lbug fetch-ort-windows fetch-model
	@mkdir -p cmd/launcher/runtime
	CGO_ENABLED=1 CGO_CFLAGS="-I/mingw64/include" CGO_CXXFLAGS="-I/mingw64/include" CGO_LDFLAGS="-L/mingw64/lib -licuuc -licuin -licudt -lstdc++" go build -tags "$(BUILD_TAGS)" -ldflags "$(LDFLAGS)" -o cmd/launcher/runtime/$(BRAND)-core.exe $(CMD)
	CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp.exe ./cmd/mcp
	GOPATH_UNIX=$$(cygpath -u "$$(go env GOPATH)") && find $$GOPATH_UNIX/pkg/mod/github.com/!ladybug!d!b/go-ladybug@$(LBUG_VERSION)/lib -maxdepth 1 -name "lbug_shared.dll" -exec cp -L {} cmd/launcher/runtime/ \;
	cp /mingw64/bin/libicuuc*.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp /mingw64/bin/libicuin*.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp /mingw64/bin/libicudt*.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp /mingw64/bin/libgcc_s_seh-1.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp /mingw64/bin/libstdc++-6.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp /mingw64/bin/libwinpthread-1.dll cmd/launcher/runtime/ 2>/dev/null || true
	cp -L $(ORT_CACHE)/onnxruntime-win-x64-$(ORT_VERSION)/lib/onnxruntime.dll cmd/launcher/runtime/
	$(call bundle_model)
	$(call bundle_ast)
	@mkdir -p $(BIN_DIR)
	CGO_ENABLED=0 go build -ldflags "$(LDFLAGS)" -o $(BIN_DIR)/$(BRAND)-windows-amd64.exe ./cmd/launcher
	rm -rf cmd/launcher/runtime/*

build-all: build-linux build-darwin build-windows




test: setup-lbug
	@LBUG_LIB="$(LBUG_MOD)/lib"; \
	if [ -f "$$LBUG_LIB/liblbug.so" ] && [ ! -f "$$LBUG_LIB/liblbug.so.0" ]; then \
		cp -L "$$LBUG_LIB/liblbug.so" "$$LBUG_LIB/liblbug.so.0"; \
	fi; \
	echo "  → Running tests with race detector (project code)…"; \
	LD_LIBRARY_PATH="$$LBUG_LIB:$$LD_LIBRARY_PATH" go test -race -coverprofile=coverage.out -covermode=atomic -p 4 \
		$$(go list ./... | grep -v "/antlr/" | grep -v "/treesitter/"); \
	echo "  → Running tests without race detector (generated parsers, appended)…"; \
	LD_LIBRARY_PATH="$$LBUG_LIB:$$LD_LIBRARY_PATH" go test -coverprofile=coverage-parsers.out -covermode=atomic -p 4 \
		$$(go list ./... | grep -E "/antlr/|/treesitter/"); \
	if [ -f coverage-parsers.out ]; then \
		tail -n +2 coverage-parsers.out >> coverage.out; \
		rm -f coverage-parsers.out; \
	fi

lint:
	golangci-lint run ./...

vulncheck:
	go run golang.org/x/vuln/cmd/govulncheck@latest ./...

ui-lint:
	cd internal/ui && npm run lint

fmt:
	gofmt -w .

vet:
	go vet $$(go list ./... | grep -v "/antlr/" | grep -v "/treesitter/")


ci: vet lint vulncheck test ui ui-lint
	@echo ""
	@echo "  ✅ All CI checks passed."
	@echo ""


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
