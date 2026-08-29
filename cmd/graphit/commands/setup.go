package commands

import (
	"bufio"
	"context"
	"fmt"
	"os"
	"os/exec"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/spf13/cobra"
	"golang.org/x/term"
)

func newSetupCmd() *cobra.Command {

	cmd := &cobra.Command{
		Use:   "setup",
		Short: "Configure " + brand.DisplayName + " (hub bucket, default IDE, default CLI)",
		Long: `Interactive setup for ` + brand.DisplayName + `.

This command configures the essential settings:
  • Hub S3 bucket (where artifacts, the registry and published stores live)
  • Bucket region, and an optional endpoint for S3-compatible servers such as MinIO
  • Default IDE (used when --ide is not explicitly provided)
  • Default CLI (used for AI fallback when API keys are missing)

Settings are stored in ~/` + brand.DotDir() + `/config.json (global config).

S3 access credentials are optional. When supplied, they are stored in the global
configuration. When left blank, the AWS default credential chain remains active:
AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY, ~/.aws/credentials, or an instance role.`,
		SilenceUsage: true,
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")
			reader := bufio.NewReader(os.Stdin)

			if _, err := exec.LookPath("git"); err != nil {
				p.Error("git is required but was not found in PATH")
				p.Detail("Install git", "https://git-scm.com/downloads")
				return fmt.Errorf("git CLI not found in PATH: %w", err)
			}

			p.Header("Welcome to %s setup", brand.DisplayName)

			bucket := promptValue(p, reader, "hub bucket name", "hub.bucket",
				config.HubBucket(), brand.DefaultHubBucket, "leave blank for local-only mode")
			if bucket == "" {
				p.StepOK("Hub bucket: local-only (no remote configured)")
			}

			if bucket != "" {
				promptValue(p, reader, "bucket region", "hub.region",
					config.HubRegion(), brand.DefaultHubRegion, "leave blank to use the AWS default chain")
				promptValue(p, reader, "S3 endpoint (MinIO and other S3-compatible servers)",
					"hub.endpoint", config.HubEndpoint(), brand.DefaultHubEndpoint, "leave blank for AWS")
				if err := promptS3Credentials(p, reader); err != nil {
					return err
				}
			}

			ideInput := promptSimple(reader, "default IDE", config.DefaultIDE())
			if err := config.SetGlobalConfigValue("ide", ideInput); err != nil {
				return fmt.Errorf("saving ide: %w", err)
			}
			p.StepOK("Default IDE: %s", ideInput)

			cliInput := promptSimple(reader, "default CLI", config.DefaultCLI())
			if err := config.SetGlobalConfigValue("cli", cliInput); err != nil {
				return fmt.Errorf("saving cli: %w", err)
			}
			p.StepOK("Default CLI: %s", cliInput)

			embeddingProvider, err := promptEmbeddingProvider(p, reader)
			if err != nil {
				return err
			}

			if _, err := promptRerankProvider(p, reader); err != nil {
				return err
			}

			p.Blank()

			if bucket != "" {
				if err := verifyHubBucket(cmd.Context(), p); err != nil {
					return err
				}
			}

			task := p.StartTask("Initialising local hub cache...")
			st, err := hub.NewS3Store(cmd.Context(), nil, nil)
			if err != nil {
				task.Fail("Hub init failed: %v", err)
				return fmt.Errorf("initializing hub: %w", err)
			}
			if err := st.SyncRegistry(cmd.Context()); err != nil {
				task.Fail("Registry sync failed (will retry on next command): %v", err)
			} else {
				task.Done("Hub cache ready at %s", st.CacheDir())
			}

			memTask := p.StartTask("Initialising memory store...")
			memStore, err := memory.NewMemoryStore()
			if err != nil {
				memTask.Fail("Memory store failed: %v", err)
				return fmt.Errorf("resolving memory store path: %w", err)
			}
			if err := memStore.EnsureInitialised(); err != nil {
				memTask.Fail("Memory store init: %v", err)
			} else {
				memTask.Done("Memory store ready at %s", memStore.Dir())
			}

			// The local embedding model is not in the binary — it is downloaded once, here,
			// into the shared cache, and ONLY when ai.embedding.provider is (still) "local".
			// A remote provider never downloads it: nothing local is ever needed to talk to
			// an HTTP API. This step is FATAL for the local provider: an installation
			// without the model is a half installation, and letting setup report success
			// would hide that until the first search came back on keywords alone with no
			// explanation.
			//
			// It is deliberately stricter than the local store initialisation above, which
			// only warns — those retry on the next command, whereas this is a fixed asset
			// the tool needs to do its job. It is also the last step on purpose: everything
			// before it has already been written, so re-running setup after fixing the
			// network costs one prompt pass and loses nothing.
			if embeddingProvider == "" || embeddingProvider == "local" {
				if modelDir, downloaded, modelErr := ensureEmbeddingModel(cmd.Context(), p); modelErr != nil {
					p.Error("Embedding model download failed: %v", modelErr)
					p.Detail("Model cache", modelDir)
					p.Step("Every other setting was saved — fix the network and run '%s setup' again", brand.BinName())
					p.Step("Behind a proxy? set HTTP_PROXY / HTTPS_PROXY. Air-gapped? place model.onnx and tokenizer.json in the cache directory above")
					return fmt.Errorf("downloading embedding model: %w", modelErr)
				} else if downloaded {
					p.StepOK("Embedding model downloaded to %s", modelDir)
				} else {
					p.StepOK("Embedding model already present at %s", modelDir)
				}
			} else {
				p.StepOK("Embedding provider is %s — no local model to download", embeddingProvider)
			}

			p.Blank()
			p.Success("Setup complete! Run '%s init' to initialize a project.", brand.BinName())

			if !config.IsModuleDisabled("daemon", nil, nil) {
				_, _ = daemon.EnsureRunning()
			}

			tracker := hub.NewEventTracker(st)
			tracker.TrackEvent("global.setup", "", nil, map[string]string{
				"ide": ideInput,
				"cli": cliInput,
			})

			return nil
		},
	}
	return cmd
}

// verifyHubBucket reaches the bucket with the credentials the AWS chain resolved. It is
// fatal on purpose: a bucket that cannot be read is an installation that will fail on its
// first artifact operation, and saying so here is the difference between one clear error and
// a confusing one much later.
func verifyHubBucket(ctx context.Context, p *output.Printer) error {
	task := p.StartTask("Verifying hub bucket access...")

	cfg := config.HubS3Config()
	store, err := s3store.New(ctx, cfg)
	if err != nil {
		task.Fail("Bucket unreachable: %v", err)
		return fmt.Errorf("configuring hub bucket: %w", err)
	}
	if err := store.EnsureBucket(ctx); err != nil {
		task.Fail("Bucket unreachable: %v", err)
		p.Detail("Bucket", cfg.Bucket)
		if cfg.Endpoint != "" {
			p.Detail("Endpoint", cfg.Endpoint)
		}
		if cfg.HasStaticCredentials() {
			p.Step("The explicit S3 credentials stored in global config were used")
		} else {
			p.Step("No explicit S3 credentials are stored; the AWS default credential chain is active")
			p.Step("Set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY, configure ~/.aws/credentials, use an instance role, or run '%s setup' again", brand.BinName())
		}
		p.Step("Wrong region is the other common cause: set it with '%s config --global hub.region <region>'", brand.BinName())
		return fmt.Errorf("reaching hub bucket %q: %w", cfg.Bucket, err)
	}

	task.Done("Hub bucket ready: %s", store.URI(""))
	return nil
}

func promptS3Credentials(p *output.Printer, reader *bufio.Reader) error {
	p.Step("S3 credentials are optional. Leave either value blank to use the AWS default credential chain (environment, ~/.aws/credentials, or an instance role).")
	fmt.Print("  Enter S3 access key ID [leave blank for AWS default chain]: ")
	accessKeyID, _ := reader.ReadString('\n')
	accessKeyID = strings.TrimSpace(accessKeyID)

	secretAccessKey, err := promptSecret(p, reader, "S3 secret access key", "leave blank for AWS default chain")
	if err != nil {
		return err
	}

	if err := config.SetGlobalS3Credentials(accessKeyID, secretAccessKey); err != nil {
		return fmt.Errorf("saving S3 credentials: %w", err)
	}
	if accessKeyID == "" || secretAccessKey == "" {
		p.StepOK("S3 credentials: AWS default credential chain")
		return nil
	}
	p.StepOK("S3 credentials: stored in global config")
	return nil
}

// promptSecret reads a masked value — a password or an API key — from the terminal, falling
// back to a plain line read when stdin is not a terminal (piped input: tests, CI, scripts have
// nothing to mask against). hint is the bracketed text shown after the label, e.g. "leave blank
// to use an environment variable instead".
func promptSecret(p *output.Printer, reader *bufio.Reader, label, hint string) (string, error) {
	fmt.Printf("  Enter %s [%s]: ", label, hint)
	var raw []byte
	var err error
	if term.IsTerminal(int(os.Stdin.Fd())) {
		raw, err = term.ReadPassword(int(os.Stdin.Fd()))
		p.Blank()
	} else {
		var line string
		line, err = reader.ReadString('\n')
		raw = []byte(line)
	}
	if err != nil {
		return "", fmt.Errorf("reading %s: %w", label, err)
	}
	return strings.TrimSpace(string(raw)), nil
}

// embeddingProviders and rerankProviders are shown in the setup prompt so the operator does
// not have to already know the valid values for ai.embedding.provider / ai.rerank.provider.
const (
	embeddingProviderChoices = "local/openai/openai-compatible/cohere/voyage/google"
	rerankProviderChoices    = "local/cohere/voyage/jina"
)

// promptEmbeddingProvider asks which embedding backend to use, saving ai.embedding.provider
// and, for anything other than "local", the model/API key/base URL that provider needs.
//
// Local stays the default and downloads nothing here — its one-time model download is the
// separate, still-mandatory-for-local step later in this command. A remote provider needs no
// download at all: it is an HTTP client, not a model on disk.
func promptEmbeddingProvider(p *output.Printer, reader *bufio.Reader) (string, error) {
	p.Blank()
	p.Step("Embedding provider: local runs a small ONNX model on this machine (nothing downloaded until the step below). OpenAI, Cohere, Voyage AI, and Google send text to that provider's API instead. openai-compatible talks to any self-hosted server using the OpenAI /v1/embeddings shape — Ollama, vLLM, LM Studio, TEI, and others.")
	provider := strings.ToLower(strings.TrimSpace(promptSimple(reader,
		fmt.Sprintf("embedding provider [%s]", embeddingProviderChoices),
		firstNonEmptyString(config.ResolveConfig("ai.embedding.provider", nil, nil), "local"))))
	if provider == "" {
		provider = "local"
	}
	if err := config.SetGlobalConfigValue("ai.embedding.provider", provider); err != nil {
		return "", fmt.Errorf("saving ai.embedding.provider: %w", err)
	}
	if provider == "local" {
		p.StepOK("Embedding provider: local")
		return provider, nil
	}

	model := promptSimple(reader, "embedding model (blank for the provider default)",
		config.ResolveConfig("ai.embedding.model", nil, nil))
	if model != "" {
		if err := config.SetGlobalConfigValue("ai.embedding.model", model); err != nil {
			return "", fmt.Errorf("saving ai.embedding.model: %w", err)
		}
	}

	if provider == "openai-compatible" {
		baseURL := promptSimple(reader, "endpoint base URL (e.g. http://localhost:11434/v1 for Ollama)",
			config.ResolveConfig("ai.embedding.base_url", nil, nil))
		if baseURL == "" {
			return "", fmt.Errorf("ai.embedding.provider=openai-compatible needs a base URL")
		}
		if err := config.SetGlobalConfigValue("ai.embedding.base_url", baseURL); err != nil {
			return "", fmt.Errorf("saving ai.embedding.base_url: %w", err)
		}
	}

	apiKey, err := promptSecret(p, reader, "API key", "leave blank to use an environment variable instead")
	if err != nil {
		return "", err
	}
	if apiKey != "" {
		if err := config.SetGlobalConfigValue("ai.embedding.api_key", apiKey); err != nil {
			return "", fmt.Errorf("saving ai.embedding.api_key: %w", err)
		}
		p.StepOK("Embedding provider: %s (API key stored)", provider)
	} else {
		p.StepOK("Embedding provider: %s (no API key stored here — set one via '%s config' or the provider's usual environment variable before using it)", provider, brand.BinName())
	}
	return provider, nil
}

// promptRerankProvider is promptEmbeddingProvider's counterpart for ai.rerank.provider.
//
// Reranking itself stays opt-in via search.rerank regardless of provider — this only decides
// WHICH backend answers when it is turned on. Local's cross-encoder (~1.04 GiB) is downloaded
// lazily on first use, never here; a remote provider never downloads anything.
func promptRerankProvider(p *output.Printer, reader *bufio.Reader) (string, error) {
	p.Blank()
	p.Step("Rerank provider: an optional second search stage (enabled separately via search.rerank), independent of the embedding provider above. local uses a cross-encoder model, downloaded on first use, not here.")
	provider := strings.ToLower(strings.TrimSpace(promptSimple(reader,
		fmt.Sprintf("rerank provider [%s]", rerankProviderChoices),
		firstNonEmptyString(config.ResolveConfig("ai.rerank.provider", nil, nil), "local"))))
	if provider == "" {
		provider = "local"
	}
	if err := config.SetGlobalConfigValue("ai.rerank.provider", provider); err != nil {
		return "", fmt.Errorf("saving ai.rerank.provider: %w", err)
	}
	if provider == "local" {
		p.StepOK("Rerank provider: local")
		return provider, nil
	}

	model := promptSimple(reader, "rerank model (blank for the provider default)",
		config.ResolveConfig("ai.rerank.model", nil, nil))
	if model != "" {
		if err := config.SetGlobalConfigValue("ai.rerank.model", model); err != nil {
			return "", fmt.Errorf("saving ai.rerank.model: %w", err)
		}
	}

	apiKey, err := promptSecret(p, reader, "API key", "leave blank to use an environment variable instead")
	if err != nil {
		return "", err
	}
	if apiKey != "" {
		if err := config.SetGlobalConfigValue("ai.rerank.api_key", apiKey); err != nil {
			return "", fmt.Errorf("saving ai.rerank.api_key: %w", err)
		}
		p.StepOK("Rerank provider: %s (API key stored)", provider)
	} else {
		p.StepOK("Rerank provider: %s (no API key stored here — set one via '%s config' or the provider's usual environment variable before using it)", provider, brand.BinName())
	}
	return provider, nil
}

func firstNonEmptyString(vals ...string) string {
	for _, v := range vals {
		if v != "" {
			return v
		}
	}
	return ""
}

// promptValue asks for one global setting, offering the current value or the compiled-in
// default, and unsets the key when the user explicitly clears it with "-".
//
// Blank input always accepts the offered default outright — it does not re-ask, since the
// default was already shown in the prompt itself and asking again is asking the same
// question twice.
func promptValue(p *output.Printer, reader *bufio.Reader, label, key, current, compiledDefault, blankHint string) string {
	fallback := current
	if fallback == "" {
		fallback = compiledDefault
	}
	if current != "" {
		p.Detail("Current "+label, current)
	}
	switch {
	case fallback != "" && current != "":
		fmt.Printf("  Enter %s [%s] (- to clear): ", label, fallback)
	case fallback != "":
		fmt.Printf("  Enter %s [%s]: ", label, fallback)
	default:
		fmt.Printf("  Enter %s [%s]: ", label, blankHint)
	}

	input, _ := reader.ReadString('\n')
	input = strings.TrimSpace(input)

	if current != "" && input == "-" {
		_ = config.UnsetGlobalConfigValue(key)
		p.StepOK("Cleared %s", label)
		return ""
	}

	if input == "" {
		switch {
		case compiledDefault != "" && current == "":
			input = compiledDefault
			p.Step("Using default: %s", input)
		case current != "":
			input = current
		default:
			return ""
		}
	}

	if err := config.SetGlobalConfigValue(key, input); err != nil {
		p.Error("saving %s: %v", key, err)
		return ""
	}
	p.StepOK("%s: %s", label, input)
	return input
}

func promptSimple(reader *bufio.Reader, label, current string) string {
	fmt.Printf("  Enter %s [%s]: ", label, current)
	input, _ := reader.ReadString('\n')
	input = strings.TrimSpace(input)
	if input == "" {
		return current
	}
	return input
}
