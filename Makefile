.PHONY: build run test check clippy fmt fmt-check ui ui-dev ui-test ui-lint native-test grammars release install clean

CARGO ?= cargo
TARGET_DIR := $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR),target)
PREFIX ?= $(HOME)/.local
DIST_DIR ?= .build/dist
HOST_OS := $(shell uname -s)
HOST_ARCH := $(shell uname -m)

ifeq ($(HOST_OS)-$(HOST_ARCH),Linux-x86_64)
PLATFORM ?= linux-amd64
else ifeq ($(HOST_OS)-$(HOST_ARCH),Darwin-arm64)
PLATFORM ?= darwin-arm64
else ifneq ($(findstring NT-,$(HOST_OS)),)
PLATFORM ?= windows-amd64
else
PLATFORM ?= unsupported
endif

build:
	$(CARGO) xtask cargo build --locked -p graphit-app -p graphit-daemon --bin graphit --bin graphit-model-server

run: build
	@bin_dir=$$(cd "$(TARGET_DIR)/debug" && pwd) && \
	  PATH="$$bin_dir:$$PATH" exec "$$bin_dir/graphit$(if $(filter windows-amd64,$(PLATFORM)),.exe,)" $(ARGS)

test:
	$(CARGO) --offline xtask $(if $(strip $(TEST)),test $(TEST),tests)

check:
	$(CARGO) xtask cargo check --locked --workspace

clippy:
	$(CARGO) xtask cargo clippy --locked --workspace --all-targets --no-deps -- -D warnings

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

ui:
	node scripts/build-rust-ui.mjs

ui-dev:
	npm --prefix ui run dev

ui-test:
	npm --prefix ui test

ui-lint:
	npm --prefix ui run lint

native-test:
	node scripts/test-native-packaging.mjs

grammars:
	$(CARGO) xtask cargo build --locked -p graphit-ast

release:
	mkdir -p "$(TARGET_DIR)"
	$(CARGO) xtask cargo build --locked --release --target-dir "$(TARGET_DIR)" -p graphit-app -p graphit-daemon --bin graphit --bin graphit-model-server --message-format=json-render-diagnostics > "$(TARGET_DIR)/release-artifacts.jsonl"
	bash scripts/package-release.sh "$(PLATFORM)" "$(TARGET_DIR)/release-artifacts.jsonl" "$(DIST_DIR)"

install: release
ifeq ($(PLATFORM),windows-amd64)
	powershell -NoProfile -File install.ps1 -Archive "$(DIST_DIR)/graphit-$(PLATFORM).tar.gz" -Dir "$(PREFIX)/bin"
else
	sh install.sh --archive "$(DIST_DIR)/graphit-$(PLATFORM).tar.gz" --dir "$(PREFIX)/bin"
endif

clean:
	$(CARGO) clean
	rm -rf .build/dist ui/dist
