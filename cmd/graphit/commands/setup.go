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

			// The embedding model is not in the binary — it is downloaded once,
			// here, into the shared cache. This step is FATAL: an installation
			// without the model is a half installation, and letting setup
			// report success would hide that until the first search came back
			// on keywords alone with no explanation.
			//
			// It is deliberately stricter than the local store initialisation
			// above, which only warns — those retry on the next command,
			// whereas this is a fixed asset the tool needs to do its job. It is
			// also the last step on purpose: everything before it has already
			// been written, so re-running setup after fixing the network costs
			// one prompt pass and loses nothing.
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

	fmt.Print("  Enter S3 secret access key [leave blank for AWS default chain]: ")
	var secretBytes []byte
	var err error
	if term.IsTerminal(int(os.Stdin.Fd())) {
		secretBytes, err = term.ReadPassword(int(os.Stdin.Fd()))
		p.Blank()
	} else {
		var secret string
		secret, err = reader.ReadString('\n')
		secretBytes = []byte(secret)
	}
	if err != nil {
		return fmt.Errorf("reading S3 secret access key: %w", err)
	}
	secretAccessKey := strings.TrimSpace(string(secretBytes))

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

// promptValue asks for one global setting, offering the current value or the compiled-in
// default, and unsets the key when the user clears it.
func promptValue(p *output.Printer, reader *bufio.Reader, label, key, current, compiledDefault, blankHint string) string {
	fallback := current
	if fallback == "" {
		fallback = compiledDefault
	}
	if current != "" {
		p.Detail("Current "+label, current)
	}
	if fallback != "" {
		fmt.Printf("  Enter %s [%s]: ", label, fallback)
	} else {
		fmt.Printf("  Enter %s [%s]: ", label, blankHint)
	}

	input, _ := reader.ReadString('\n')
	input = strings.TrimSpace(input)

	if input == "" {
		switch {
		case compiledDefault != "" && current == "":
			input = compiledDefault
			p.Step("Using default: %s", input)
		case current != "":
			fmt.Printf("  Keep current %s %q? [Y/n]: ", label, current)
			keep, _ := reader.ReadString('\n')
			keep = strings.TrimSpace(strings.ToLower(keep))
			if keep == "" || keep == "y" || keep == "yes" {
				input = current
				p.Step("Keeping current: %s", input)
			} else {
				_ = config.UnsetGlobalConfigValue(key)
				p.StepOK("Cleared %s", label)
				return ""
			}
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
