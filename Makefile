.PHONY: build build-all install clean fmt vet run ui ui-dev setup-lbug \
       fetch-ort-linux fetch-ort-darwin fetch-ort-windows fetch-model lint \
       ui-lint ci check test build-windows-native

MODULE   := github.com/graphit-labs/graphit-code
CMD      := ./cmd/graphit
BIN_DIR  := .build

BRAND        ?= graphit
DISPLAY_NAME ?= Graphit Code: A Powerful Agent Harness for Enterprise Software Ecosystems
VERSION      ?= v0.1.13
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



ui:
	cd internal/ui && npm ci --prefer-offline
	cd internal/ui && npm run build

ui-dev:
	cd internal/ui && npm run dev

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
	sudo cp $(BIN_DIR)/$(BRAND)-linux-amd64 /usr/local/bin/$(BRAND)

build-linux: ui setup-lbug fetch-ort-linux fetch-model
	@mkdir -p cmd/launcher/runtime
	GOOS=linux GOARCH=amd64 CGO_ENABLED=1 go build -tags "$(BUILD_TAGS)" -ldflags "$(LDFLAGS)" -o cmd/launcher/runtime/$(BRAND)-core $(CMD)
	GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp ./cmd/mcpproxy
	find $$(go env GOPATH)/pkg/mod/github.com/!ladybug!d!b/go-ladybug*/lib/dynamic/linux-amd64 -name "liblbug.so" -exec cp -L {} cmd/launcher/runtime/ \;
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
	GOOS=darwin GOARCH=arm64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp ./cmd/mcpproxy
	find $$(go env GOPATH)/pkg/mod/github.com/!ladybug!d!b/go-ladybug*/lib/dynamic/darwin -name "liblbug.dylib" -exec cp -L {} cmd/launcher/runtime/ \;
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
	GOOS=windows GOARCH=amd64 CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp.exe ./cmd/mcpproxy
	find $$(go env GOPATH)/pkg/mod/github.com/!ladybug!d!b/go-ladybug*/lib/dynamic/windows -name "lbug_shared.dll" -exec cp -L {} cmd/launcher/runtime/ \;
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
	CGO_ENABLED=0 go build -ldflags "$(LDFLAGS) -s -w" -o cmd/launcher/runtime/$(BRAND)-mcp.exe ./cmd/mcpproxy
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
	@mkdir -p $(BIN_DIR)
	CGO_ENABLED=0 go build -ldflags "$(LDFLAGS)" -o $(BIN_DIR)/$(BRAND)-windows-amd64.exe ./cmd/launcher
	rm -rf cmd/launcher/runtime/*

build-all: build-linux build-darwin build-windows

test: setup-lbug
	@LBUG_LIB="$(LBUG_MOD)/lib/dynamic/linux-amd64"; \
	if [ -f "$$LBUG_LIB/liblbug.so" ] && [ ! -f "$$LBUG_LIB/liblbug.so.0" ]; then \
		cp -L "$$LBUG_LIB/liblbug.so" "$$LBUG_LIB/liblbug.so.0"; \
	fi; \
	echo "  → Running tests with race detector (project code)…"; \
	LD_LIBRARY_PATH="$$LBUG_LIB:$$LD_LIBRARY_PATH" go test -race -cover -p 4 \
		$$(go list ./... | grep -v "/antlr/" | grep -v "/treesitter/"); \
	echo "  → Running tests without race detector (generated parsers)…"; \
	LD_LIBRARY_PATH="$$LBUG_LIB:$$LD_LIBRARY_PATH" go test -cover -p 4 \
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

