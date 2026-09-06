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
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/spf13/cobra"
	"golang.org/x/term"
)

func newSetupCmd() *cobra.Command {

	var answers setupAnswers

	cmd := &cobra.Command{
		Use:   "setup",
		Short: "Configure " + brand.DisplayName + " (username, hub bucket, default IDE, default CLI)",
		Long: `Interactive setup for ` + brand.DisplayName + `.

This command configures the essential settings:
  • Username used for Hub access (empty means anonymous)
  • Hub S3 bucket (where artifacts, the registry and published stores live)
  • Bucket region, and an optional endpoint for S3-compatible servers such as MinIO
  • Default IDE (used when --ide is not explicitly provided)
  • Default CLI (used for AI fallback when API keys are missing)

Settings are stored in ~/` + brand.DotDir() + `/config.json (global config).

S3 access credentials are optional. When supplied, they are stored in the global
configuration. When left blank, the AWS default credential chain remains active:
AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY, ~/.aws/credentials, or an instance role.

Every question also has a flag, and a question whose flag was supplied is not
asked. Answer one thing on the command line and setup asks about the rest;
answer everything it reaches and it needs no terminal at all, which is how it
runs in a container or a pipeline. There is no separate non-interactive switch:
silence is the consequence of having answered, not a mode.

  # one answer given, the rest still asked
  ` + brand.BinName() + ` setup --ide cursor

  # nothing left to ask: local-only hub, so region, endpoint and credentials
  # are never reached, and both providers are local
  ` + brand.BinName() + ` setup --username "" --hub-bucket "" --ide cursor --cli cursor-agent \
      --embedding-provider local --rerank-provider local

An empty value is an answer: it clears that key. Omitting the flag entirely
leaves the key untouched.

Nothing here is softened by answering in advance: an unreachable hub bucket and,
for the local embedding provider, a failed model download both fail the command
rather than leaving a half installation reporting success.`,
		SilenceUsage: true,
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")
			reader := bufio.NewReader(os.Stdin)
			answers.bind(cmd)

			if _, err := exec.LookPath("git"); err != nil {
				p.Error("git is required but was not found in PATH")
				p.Detail("Install git", "https://git-scm.com/downloads")
				return fmt.Errorf("git CLI not found in PATH: %w", err)
			}

			p.Header("Welcome to %s setup", brand.DisplayName)

			currentUsername, _, err := config.GetGlobalConfigValue("hub.subject.user")
			if err != nil {
				return fmt.Errorf("reading hub.subject.user: %w", err)
			}
			if _, err := answers.username.hubUsername(p, reader, currentUsername); err != nil {
				return err
			}

			bucket, err := answers.hubBucket.value(p, reader, "hub bucket name", "hub.bucket",
				config.HubBucket(), brand.DefaultHubBucket, "leave blank for local-only mode")
			if err != nil {
				return err
			}
			if bucket == "" {
				p.StepOK("Hub bucket: local-only (no remote configured)")
			}

			if bucket != "" {
				if _, err := answers.hubRegion.value(p, reader, "bucket region", "hub.region",
					config.HubRegion(), brand.DefaultHubRegion, "leave blank to use the AWS default chain"); err != nil {
					return err
				}
				if _, err := answers.hubEndpoint.value(p, reader,
					"S3 endpoint (MinIO and other S3-compatible servers)",
					"hub.endpoint", config.HubEndpoint(), brand.DefaultHubEndpoint, "leave blank for AWS"); err != nil {
					return err
				}
				if err := promptS3Credentials(p, reader, answers); err != nil {
					return err
				}
			}

			ideInput := answers.ide.simple(reader, "default IDE", config.DefaultIDE())
			if err := config.SetGlobalConfigValue("ide", ideInput); err != nil {
				return fmt.Errorf("saving ide: %w", err)
			}
			p.StepOK("Default IDE: %s", ideInput)

			cliInput := answers.cli.simple(reader, "default CLI", config.DefaultCLI())
			if err := config.SetGlobalConfigValue("cli", cliInput); err != nil {
				return fmt.Errorf("saving cli: %w", err)
			}
			p.StepOK("Default CLI: %s", cliInput)

			embeddingProvider, err := promptEmbeddingProvider(p, reader, answers)
			if err != nil {
				return err
			}

			if _, err := promptRerankProvider(p, reader, answers); err != nil {
				return err
			}

			p.Blank()

			if bucket != "" {
				if err := verifyHubBucket(cmd.Context(), p); err != nil {
					return err
				}
			}

			task := p.StartTask("Checking hub storage...")
			st, err := hub.NewS3Store(cmd.Context(), nil, nil)
			if err != nil {
				task.Fail("Hub init failed: %v", err)
				return fmt.Errorf("initializing hub: %w", err)
			}
			if err := st.EnsureReachable(cmd.Context()); err != nil {
				task.Fail("Hub storage check failed: %v", err)
			} else {
				task.Done("Hub storage ready")
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

			return nil
		},
	}

	answers.register(cmd)
	return cmd
}

type setupAnswer struct {
	given string
	set   bool
}

type setupAnswers struct {
	username    setupAnswer
	hubBucket   setupAnswer
	hubRegion   setupAnswer
	hubEndpoint setupAnswer
	accessKeyID setupAnswer
	secretKey   setupAnswer

	ide setupAnswer
	cli setupAnswer

	embeddingProvider setupAnswer
	embeddingModel    setupAnswer
	embeddingBaseURL  setupAnswer
	embeddingAPIKey   setupAnswer

	rerankProvider setupAnswer
	rerankModel    setupAnswer
	rerankAPIKey   setupAnswer
}

func (a *setupAnswers) fields() map[string]*setupAnswer {
	return map[string]*setupAnswer{
		"username":              &a.username,
		"hub-bucket":            &a.hubBucket,
		"hub-region":            &a.hubRegion,
		"hub-endpoint":          &a.hubEndpoint,
		"hub-access-key-id":     &a.accessKeyID,
		"hub-secret-access-key": &a.secretKey,
		"ide":                   &a.ide,
		"cli":                   &a.cli,
		"embedding-provider":    &a.embeddingProvider,
		"embedding-model":       &a.embeddingModel,
		"embedding-base-url":    &a.embeddingBaseURL,
		"embedding-api-key":     &a.embeddingAPIKey,
		"rerank-provider":       &a.rerankProvider,
		"rerank-model":          &a.rerankModel,
		"rerank-api-key":        &a.rerankAPIKey,
	}
}

func (a *setupAnswers) register(cmd *cobra.Command) {
	f := cmd.Flags()

	f.String("username", "", "Hub username (empty selects anonymous)")
	f.String("hub-bucket", "", "Hub S3 bucket (empty value clears it and selects local-only mode)")
	f.String("hub-region", "", "Hub bucket region (empty value clears it)")
	f.String("hub-endpoint", "", "S3 endpoint for MinIO and other S3-compatible servers (empty value clears it)")
	f.String("hub-access-key-id", "", "Explicit S3 access key ID (both credential flags are needed; either one empty clears the pair)")
	f.String("hub-secret-access-key", "", secretFlagUsage("S3 secret access key", "hub.secret_access_key"))

	f.String("ide", "", "Default IDE")
	f.String("cli", "", "Default CLI used for AI fallback")

	f.String("embedding-provider", "", "Embedding provider ["+embeddingProviderChoices+"]")
	f.String("embedding-model", "", "Embedding model (empty for the provider default)")
	f.String("embedding-base-url", "", "Endpoint base URL, required by embedding-provider=openai-compatible")
	f.String("embedding-api-key", "", secretFlagUsage("Embedding provider API key", "ai.embedding.api_key"))

	f.String("rerank-provider", "", "Rerank provider ["+rerankProviderChoices+"]")
	f.String("rerank-model", "", "Rerank model (empty for the provider default)")
	f.String("rerank-api-key", "", secretFlagUsage("Rerank provider API key", "ai.rerank.api_key"))
}

func (answer setupAnswer) hubUsername(p *output.Printer, reader *bufio.Reader, current string) (string, error) {
	current = strings.TrimSpace(current)
	if !answer.set {
		if current != "" {
			p.Detail("Current username", current)
		}
		fmt.Print("  Enter username [leave blank for anonymous]: ")
		raw, _ := reader.ReadString('\n')
		answer.given = raw
	}

	username := strings.TrimSpace(answer.given)
	if username == "" || hubaccess.IsAnonymousUserID(username) {
		if err := config.UnsetGlobalConfigValue("hub.subject.user"); err != nil {
			return "", fmt.Errorf("selecting anonymous user: %w", err)
		}
		p.StepOK("Username: %s", hubaccess.AnonymousUserID)
		return hubaccess.AnonymousUserID, nil
	}
	if err := hubaccess.ValidateSubjectID("user", username); err != nil {
		return "", err
	}
	if err := config.SetGlobalConfigValue("hub.subject.user", username); err != nil {
		return "", fmt.Errorf("saving hub.subject.user: %w", err)
	}
	p.StepOK("Username: %s", username)
	return username, nil
}

func secretFlagUsage(label, configKey string) string {
	return label + " (prefer " + config.ConfigEnvVar(configKey) + "; a value passed here is stored in the global config in plain text)"
}

func (a *setupAnswers) bind(cmd *cobra.Command) {
	for name, answer := range a.fields() {
		value, err := cmd.Flags().GetString(name)
		if err != nil {
			continue
		}
		answer.given = value
		answer.set = cmd.Flags().Changed(name)
	}
}

func (answer setupAnswer) value(
	p *output.Printer, reader *bufio.Reader,
	label, key, current, compiledDefault, blankHint string,
) (string, error) {
	if !answer.set {
		return promptValue(p, reader, label, key, current, compiledDefault, blankHint), nil
	}

	resolved := strings.TrimSpace(answer.given)
	if resolved == "" {
		if current != "" {
			if err := config.UnsetGlobalConfigValue(key); err != nil {
				return "", fmt.Errorf("clearing %s: %w", key, err)
			}
			p.StepOK("Cleared %s", label)
		}
		return "", nil
	}

	if err := config.SetGlobalConfigValue(key, resolved); err != nil {
		return "", fmt.Errorf("saving %s: %w", key, err)
	}
	p.StepOK("%s: %s", label, resolved)
	return resolved, nil
}

func (answer setupAnswer) simple(reader *bufio.Reader, label, current string) string {
	if !answer.set {
		return promptSimple(reader, label, current)
	}
	if trimmed := strings.TrimSpace(answer.given); trimmed != "" {
		return trimmed
	}
	return current
}

// secret is promptSecret for a setting that has a flag, and it never echoes the value.
//
// An explicitly empty flag is how a scripted run says "do not store a key here" without being
// asked — the provider then reads its own environment variable at run time, which is where a
// credential belongs.
func (answer setupAnswer) secret(
	p *output.Printer, reader *bufio.Reader, label, hint string,
) (string, error) {
	if answer.set {
		return strings.TrimSpace(answer.given), nil
	}
	return promptSecret(p, reader, label, hint)
}

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

// promptS3Credentials asks for the optional explicit credential pair and stores it, or clears
// it, as a pair.
//
// NOTE: a blank answer CLEARS a stored pair, whether it arrived from the prompt or from an
// explicitly empty flag. That is deliberate for a value that only means anything as a pair:
// half a credential is not a credential, and leaving one half behind would authenticate with
// something the operator did not ask for. Supplying one flag and not the other still asks about
// the missing half rather than assuming it.
func promptS3Credentials(p *output.Printer, reader *bufio.Reader, answers setupAnswers) error {
	var accessKeyID string
	if answers.accessKeyID.set {
		accessKeyID = strings.TrimSpace(answers.accessKeyID.given)
	} else {
		p.Step("S3 credentials are optional. Leave either value blank to use the AWS default credential chain (environment, ~/.aws/credentials, or an instance role).")
		fmt.Print("  Enter S3 access key ID [leave blank for AWS default chain]: ")
		raw, _ := reader.ReadString('\n')
		accessKeyID = strings.TrimSpace(raw)
	}

	secretAccessKey, err := answers.secretKey.secret(p, reader,
		"S3 secret access key", "leave blank for AWS default chain")
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

const (
	embeddingProviderChoices = "local/openai/openai-compatible/cohere/voyage/google"
	rerankProviderChoices    = "local/cohere/voyage/jina"
)

func promptEmbeddingProvider(p *output.Printer, reader *bufio.Reader, answers setupAnswers) (string, error) {
	p.Blank()
	if !answers.embeddingProvider.set {
		p.Step("Embedding provider: local runs a small ONNX model on this machine (nothing downloaded until the step below). OpenAI, Cohere, Voyage AI, and Google send text to that provider's API instead. openai-compatible talks to any self-hosted server using the OpenAI /v1/embeddings shape — Ollama, vLLM, LM Studio, TEI, and others.")
	}
	provider := strings.ToLower(answers.embeddingProvider.simple(reader,
		fmt.Sprintf("embedding provider [%s]", embeddingProviderChoices),
		firstNonEmptyString(config.ResolveConfig("ai.embedding.provider", nil, nil), "local")))
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

	model := answers.embeddingModel.simple(reader, "embedding model (blank for the provider default)",
		config.ResolveConfig("ai.embedding.model", nil, nil))
	if model != "" {
		if err := config.SetGlobalConfigValue("ai.embedding.model", model); err != nil {
			return "", fmt.Errorf("saving ai.embedding.model: %w", err)
		}
	}

	if provider == "openai-compatible" {
		baseURL := answers.embeddingBaseURL.simple(reader,
			"endpoint base URL (e.g. http://localhost:11434/v1 for Ollama)",
			config.ResolveConfig("ai.embedding.base_url", nil, nil))
		if baseURL == "" {
			return "", fmt.Errorf("ai.embedding.provider=openai-compatible needs a base URL")
		}
		if err := config.SetGlobalConfigValue("ai.embedding.base_url", baseURL); err != nil {
			return "", fmt.Errorf("saving ai.embedding.base_url: %w", err)
		}
	}

	apiKey, err := answers.embeddingAPIKey.secret(p, reader,
		"API key", "leave blank to use an environment variable instead")
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
func promptRerankProvider(p *output.Printer, reader *bufio.Reader, answers setupAnswers) (string, error) {
	p.Blank()
	if !answers.rerankProvider.set {
		p.Step("Rerank provider: an optional second search stage (enabled separately via search.rerank), independent of the embedding provider above. local uses a cross-encoder model, downloaded on first use, not here.")
	}
	provider := strings.ToLower(answers.rerankProvider.simple(reader,
		fmt.Sprintf("rerank provider [%s]", rerankProviderChoices),
		firstNonEmptyString(config.ResolveConfig("ai.rerank.provider", nil, nil), "local")))
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

	model := answers.rerankModel.simple(reader, "rerank model (blank for the provider default)",
		config.ResolveConfig("ai.rerank.model", nil, nil))
	if model != "" {
		if err := config.SetGlobalConfigValue("ai.rerank.model", model); err != nil {
			return "", fmt.Errorf("saving ai.rerank.model: %w", err)
		}
	}

	apiKey, err := answers.rerankAPIKey.secret(p, reader,
		"API key", "leave blank to use an environment variable instead")
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
