package commands

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
	"golang.org/x/term"
)

var nonInteractive bool

type providerOptions struct {
	typeName, issuer, clientID, clientSecret, tokenAuthMethod, scopes, redirectURI string
	usernameClaim, organizationClaim, teamsClaim                                   string
	authParams                                                                     map[string]string
	mcpAudience, mcpResource                                                       string
	brokerEndpoint, brokerAudience, brokerResource, brokerTokenStrategy            string
	brokerTokenExchangeEndpoint                                                    string
	embeddingMode, embeddingProtocol, embeddingEndpoint, embeddingModel            string
	embeddingDimensions                                                            int
	rerankMode, rerankProtocol, rerankEndpoint, rerankModel                        string
	rerankDimensions                                                               int
	embeddingDevice, embeddingDeviceID, rerankDevice, rerankDeviceID               string
	s3Bucket, s3Region, s3Endpoint, s3Prefix, s3CredentialSource                   string
	stsEndpoint, stsRoleARN, stsSessionName                                        string
	stsDuration                                                                    int32
	clearBroker, clearSTS, allowAWSChain                                           bool
	stsUseAccessToken                                                              bool
	allowBrokerAnonymous                                                           bool
}

func newProviderCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "provider", Short: "Manage reusable named authentication providers"}
	cmd.AddCommand(newProviderAddCmd(), newProviderListCmd(), newProviderShowCmd(), newProviderUpdateCmd(), newProviderRemoveCmd())
	return cmd
}

func newProviderAddCmd() *cobra.Command {
	var options providerOptions
	cmd := &cobra.Command{Use: "add <name>", Short: "Add a local, OIDC, or Graphit Broker provider", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		reader := bufio.NewReader(os.Stdin)
		if options.typeName == "" {
			value, err := inputRequiredValue(reader, "provider type (local/oidc/broker)")
			if err != nil {
				return err
			}
			options.typeName = value
		}
		provider, err := providerFromOptions(cmd, reader, args[0], options, nil)
		if err != nil {
			return err
		}
		store, err := auth.Open()
		if err != nil {
			return err
		}
		if err := store.AddProvider(provider); err != nil {
			return err
		}
		output.NewPrinter("").Success("Provider %s added", provider.Name)
		return nil
	}}
	registerProviderFlags(cmd, &options, true)
	return cmd
}

func newProviderUpdateCmd() *cobra.Command {
	var options providerOptions
	cmd := &cobra.Command{Use: "update <name>", Short: "Update a provider and invalidate its existing login sessions", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		store, err := auth.Open()
		if err != nil {
			return err
		}
		state, err := store.Load()
		if err != nil {
			return err
		}
		current, ok := state.Providers[args[0]]
		if !ok {
			return fmt.Errorf("provider %q does not exist", args[0])
		}
		provider, err := providerFromOptions(cmd, bufio.NewReader(os.Stdin), args[0], options, &current)
		if err != nil {
			return err
		}
		if err := store.UpdateProvider(provider); err != nil {
			return err
		}
		output.NewPrinter("").Success("Provider %s updated; dependent profiles must log in again", provider.Name)
		return nil
	}}
	registerProviderFlags(cmd, &options, false)
	return cmd
}

func registerProviderFlags(cmd *cobra.Command, o *providerOptions, add bool) {
	cmd.Flags().StringVar(&o.typeName, "type", "", "Provider type: local, oidc, or broker")
	cmd.Flags().StringVar(&o.issuer, "issuer", "", "OIDC issuer URL")
	cmd.Flags().StringVar(&o.clientID, "client-id", "", "OIDC client ID")
	cmd.Flags().StringVar(&o.clientSecret, "client-secret", "", "OIDC client secret stored in the restricted authentication file")
	cmd.Flags().StringVar(&o.tokenAuthMethod, "token-auth-method", "", "OIDC token auth: none, client_secret_post, or client_secret_basic")
	cmd.Flags().StringVar(&o.scopes, "scopes", "", "Comma-separated OIDC scopes")
	cmd.Flags().StringVar(&o.redirectURI, "redirect-uri", "", "OIDC HTTP loopback redirect URI")
	cmd.Flags().StringVar(&o.usernameClaim, "username-claim", "", "Verified claim used as username")
	cmd.Flags().StringVar(&o.organizationClaim, "organization-claim", "", "Verified claim used as organization")
	cmd.Flags().StringVar(&o.teamsClaim, "teams-claim", "", "Verified string or string-array claim used as teams")
	cmd.Flags().StringToStringVar(&o.authParams, "auth-param", nil, "Additional OIDC authorization parameter (key=value, repeatable)")
	cmd.Flags().StringVar(&o.mcpAudience, "mcp-audience", "", "OIDC audience requested for the daemon MCP listener")
	cmd.Flags().StringVar(&o.mcpResource, "mcp-resource", "", "OAuth resource requested for the daemon MCP listener")
	cmd.Flags().StringVar(&o.brokerEndpoint, "broker-endpoint", "", "Graphit capability broker base URL (requires broker embedding and rerank modes)")
	cmd.Flags().StringVar(&o.brokerAudience, "broker-audience", "", "OIDC audience requested for the broker")
	cmd.Flags().StringVar(&o.brokerResource, "broker-resource", "", "OAuth resource requested for the broker")
	cmd.Flags().StringVar(&o.brokerTokenStrategy, "broker-token-strategy", "", "Broker bearer strategy: relay or token-exchange")
	cmd.Flags().StringVar(&o.brokerTokenExchangeEndpoint, "broker-token-exchange-endpoint", "", "RFC 8693 token endpoint (defaults to OIDC discovery token_endpoint)")
	cmd.Flags().BoolVar(&o.allowBrokerAnonymous, "broker-allow-anonymous", false, "Allow profiles without a broker bearer credential")
	cmd.Flags().StringVar(&o.embeddingMode, "embedding-mode", "", "Embedding mode: local, direct, broker, or disabled")
	cmd.Flags().StringVar(&o.embeddingProtocol, "embedding-protocol", "", "Direct embedding protocol")
	cmd.Flags().StringVar(&o.embeddingEndpoint, "embedding-endpoint", "", "Direct embedding endpoint base URL")
	cmd.Flags().StringVar(&o.embeddingModel, "embedding-model", "", "Direct embedding model")
	cmd.Flags().IntVar(&o.embeddingDimensions, "embedding-dimensions", 0, "Direct embedding vector dimensions")
	cmd.Flags().StringVar(&o.embeddingDevice, "embedding-device", "", "Local embedding ONNX device: auto (accelerator with CPU recovery), cpu, cuda, or coreml")
	cmd.Flags().StringVar(&o.embeddingDeviceID, "embedding-device-id", "", "Zero-based CUDA device ID for local embedding")
	cmd.Flags().StringVar(&o.rerankMode, "rerank-mode", "", "Rerank mode: local, direct, broker, or disabled")
	cmd.Flags().StringVar(&o.rerankProtocol, "rerank-protocol", "", "Direct rerank protocol")
	cmd.Flags().StringVar(&o.rerankEndpoint, "rerank-endpoint", "", "Direct rerank endpoint base URL")
	cmd.Flags().StringVar(&o.rerankModel, "rerank-model", "", "Direct rerank model")
	cmd.Flags().IntVar(&o.rerankDimensions, "rerank-dimensions", 0, "Vector dimensions for embedding-simulated direct rerank")
	cmd.Flags().StringVar(&o.rerankDevice, "rerank-device", "", "Local rerank ONNX device: auto, cpu, cuda, or coreml")
	cmd.Flags().StringVar(&o.rerankDeviceID, "rerank-device-id", "", "Zero-based CUDA device ID for local rerank")
	cmd.Flags().StringVar(&o.s3Bucket, "s3-bucket", "", "S3 bucket")
	cmd.Flags().StringVar(&o.s3Region, "s3-region", "", "S3 region")
	cmd.Flags().StringVar(&o.s3Endpoint, "s3-endpoint", "", "S3-compatible endpoint")
	cmd.Flags().StringVar(&o.s3Prefix, "s3-prefix", "", "S3 key prefix")
	cmd.Flags().StringVar(&o.s3CredentialSource, "s3-credential-source", "", "S3 credentials: login, aws-chain, or sts (broker providers always use broker STS)")
	cmd.Flags().BoolVar(&o.allowAWSChain, "allow-aws-credential-chain", false, "Allow a local login to use the AWS credential chain")
	cmd.Flags().StringVar(&o.stsEndpoint, "sts-endpoint", "", "AWS-compatible STS endpoint")
	cmd.Flags().StringVar(&o.stsRoleARN, "sts-role-arn", "", "Role ARN for AssumeRoleWithWebIdentity")
	cmd.Flags().StringVar(&o.stsSessionName, "sts-session-name", "", "STS role session name")
	cmd.Flags().Int32Var(&o.stsDuration, "sts-duration", 0, "STS credential duration in seconds")
	cmd.Flags().BoolVar(&o.stsUseAccessToken, "sts-use-access-token", false, "Exchange the access token instead of the ID token")
	if !add {
		cmd.Flags().BoolVar(&o.clearSTS, "clear-sts", false, "Remove STS exchange configuration")
		cmd.Flags().BoolVar(&o.clearBroker, "clear-broker", false, "Remove broker configuration")
	}
}

func providerFromOptions(cmd *cobra.Command, reader *bufio.Reader, name string, o providerOptions, current *auth.Provider) (auth.Provider, error) {
	var p auth.Provider
	if current != nil {
		p = *current
	} else {
		p = auth.Provider{Name: name}
	}
	setString := func(flag string, target *string, value string) {
		if current == nil || cmd.Flags().Changed(flag) {
			*target = strings.TrimSpace(value)
		}
	}
	if current == nil || cmd.Flags().Changed("type") {
		p.Type = auth.ProviderType(strings.ToLower(strings.TrimSpace(o.typeName)))
	}
	switch p.Type {
	case auth.ProviderLocal:
		if cmd.Flags().Changed("mcp-audience") || cmd.Flags().Changed("mcp-resource") {
			return p, errors.New("MCP audience and resource require an OIDC provider")
		}
		if p.Local == nil {
			p.Local = &auth.LocalConfig{}
		}
		p.OIDC = nil
		p.STS = nil
		if current == nil || cmd.Flags().Changed("allow-aws-credential-chain") {
			p.Local.AllowAWSCredentialChain = o.allowAWSChain
		}
	case auth.ProviderOIDC:
		if p.OIDC == nil {
			p.OIDC = &auth.OIDCConfig{}
		}
		p.Local = nil
		setString("issuer", &p.OIDC.Issuer, o.issuer)
		setString("client-id", &p.OIDC.ClientID, o.clientID)
		setString("client-secret", &p.OIDC.ClientSecret, o.clientSecret)
		setString("token-auth-method", &p.OIDC.TokenAuthMethod, o.tokenAuthMethod)
		setString("redirect-uri", &p.OIDC.RedirectURI, o.redirectURI)
		setString("username-claim", &p.OIDC.UsernameClaim, o.usernameClaim)
		setString("organization-claim", &p.OIDC.OrganizationClaim, o.organizationClaim)
		setString("teams-claim", &p.OIDC.TeamsClaim, o.teamsClaim)
		setString("mcp-audience", &p.OIDC.MCPAudience, o.mcpAudience)
		setString("mcp-resource", &p.OIDC.MCPResource, o.mcpResource)
		if current == nil || cmd.Flags().Changed("scopes") {
			p.OIDC.Scopes = splitCSV(o.scopes)
		}
		if current == nil || cmd.Flags().Changed("auth-param") {
			p.OIDC.AuthParams = cloneStringMap(o.authParams)
		}
		if p.OIDC.Issuer == "" {
			v, e := inputRequiredValue(reader, "OIDC issuer")
			if e != nil {
				return p, e
			}
			p.OIDC.Issuer = v
		}
		if p.OIDC.ClientID == "" {
			v, e := inputRequiredValue(reader, "OIDC client ID")
			if e != nil {
				return p, e
			}
			p.OIDC.ClientID = v
		}
		if p.OIDC.UsernameClaim == "" {
			if nonInteractive {
				return p, errors.New("--username-claim is required in non-interactive mode")
			}
			p.OIDC.UsernameClaim = "preferred_username"
		}
	case auth.ProviderBroker:
		if cmd.Flags().Changed("mcp-audience") || cmd.Flags().Changed("mcp-resource") {
			return p, errors.New("broker providers validate daemon MCP tokens through Broker userinfo and do not accept MCP audience or resource flags")
		}
		if cmd.Flags().Changed("issuer") || cmd.Flags().Changed("client-id") || cmd.Flags().Changed("client-secret") || cmd.Flags().Changed("token-auth-method") || cmd.Flags().Changed("scopes") || cmd.Flags().Changed("redirect-uri") || cmd.Flags().Changed("username-claim") || cmd.Flags().Changed("organization-claim") || cmd.Flags().Changed("teams-claim") || cmd.Flags().Changed("auth-param") {
			return p, errors.New("broker providers discover login from the broker and do not accept upstream OIDC flags")
		}
		p.Local, p.OIDC = nil, nil
		p.STS = nil
		if p.Broker == nil {
			p.Broker = &auth.BrokerConfig{}
		}
	}
	brokerChanged := cmd.Flags().Changed("broker-endpoint") || cmd.Flags().Changed("broker-audience") || cmd.Flags().Changed("broker-resource") || cmd.Flags().Changed("broker-token-strategy") || cmd.Flags().Changed("broker-token-exchange-endpoint") || cmd.Flags().Changed("broker-allow-anonymous")
	if current == nil {
		brokerChanged = o.brokerEndpoint != "" || o.brokerAudience != "" || o.brokerResource != "" || o.brokerTokenStrategy != "" || o.brokerTokenExchangeEndpoint != ""
	}
	if o.clearBroker && brokerChanged {
		return p, errors.New("--clear-broker cannot be combined with broker configuration flags")
	}
	if o.clearBroker {
		if p.Type == auth.ProviderBroker {
			return p, errors.New("a broker provider cannot clear its broker endpoint")
		}
		p.Broker = nil
	} else if brokerChanged {
		if p.Broker == nil {
			p.Broker = &auth.BrokerConfig{}
		}
		setString("broker-endpoint", &p.Broker.Endpoint, o.brokerEndpoint)
		setString("broker-audience", &p.Broker.Audience, o.brokerAudience)
		setString("broker-resource", &p.Broker.Resource, o.brokerResource)
		setString("broker-token-strategy", &p.Broker.TokenStrategy, o.brokerTokenStrategy)
		setString("broker-token-exchange-endpoint", &p.Broker.TokenExchangeEndpoint, o.brokerTokenExchangeEndpoint)
		if current == nil || cmd.Flags().Changed("broker-allow-anonymous") {
			p.Broker.AllowAnonymous = o.allowBrokerAnonymous
		}
	}
	if p.Type == auth.ProviderBroker {
		if cmd.Flags().Changed("broker-audience") || cmd.Flags().Changed("broker-resource") || cmd.Flags().Changed("broker-token-exchange-endpoint") || cmd.Flags().Changed("broker-allow-anonymous") || (cmd.Flags().Changed("broker-token-strategy") && strings.TrimSpace(o.brokerTokenStrategy) != "relay") {
			return p, errors.New("broker providers use broker-issued relay tokens and do not accept audience, resource, token-exchange, or anonymous flags")
		}
		if p.Broker.Endpoint == "" {
			value, inputErr := inputRequiredValue(reader, "Graphit Broker endpoint")
			if inputErr != nil {
				return p, inputErr
			}
			p.Broker.Endpoint = value
		}
		p.Broker.TokenStrategy = "relay"
		p.Broker.Audience, p.Broker.Resource, p.Broker.TokenExchangeEndpoint = "", "", ""
		p.Broker.AllowAnonymous = false
		p.S3 = auth.S3Config{CredentialSource: "broker"}
	}
	setString("embedding-mode", (*string)(&p.AI.Embedding.Mode), o.embeddingMode)
	setString("embedding-protocol", &p.AI.Embedding.Protocol, o.embeddingProtocol)
	setString("embedding-endpoint", &p.AI.Embedding.Endpoint, o.embeddingEndpoint)
	setString("embedding-model", &p.AI.Embedding.Model, o.embeddingModel)
	if current == nil || cmd.Flags().Changed("embedding-dimensions") {
		p.AI.Embedding.Dimensions = o.embeddingDimensions
	}
	setString("rerank-mode", (*string)(&p.AI.Rerank.Mode), o.rerankMode)
	setString("rerank-protocol", &p.AI.Rerank.Protocol, o.rerankProtocol)
	setString("rerank-endpoint", &p.AI.Rerank.Endpoint, o.rerankEndpoint)
	setString("rerank-model", &p.AI.Rerank.Model, o.rerankModel)
	if current == nil || cmd.Flags().Changed("rerank-dimensions") {
		p.AI.Rerank.Dimensions = o.rerankDimensions
	}
	if current == nil {
		if p.Type == auth.ProviderBroker {
			if p.AI.Embedding.Mode == "" {
				p.AI.Embedding.Mode = auth.ServiceBroker
			}
			if p.AI.Rerank.Mode == "" {
				p.AI.Rerank.Mode = auth.ServiceBroker
			}
		} else if p.AI.Embedding.Mode == "" {
			p.AI.Embedding.Mode = auth.ServiceLocal
		}
		if p.Type != auth.ProviderBroker && p.AI.Rerank.Mode == "" {
			p.AI.Rerank.Mode = auth.ServiceLocal
		}
	}
	if cmd.Flags().Changed("embedding-mode") && p.AI.Embedding.Mode != auth.ServiceDirect {
		if cmd.Flags().Changed("embedding-protocol") || cmd.Flags().Changed("embedding-endpoint") || cmd.Flags().Changed("embedding-model") || cmd.Flags().Changed("embedding-dimensions") {
			return p, errors.New("direct embedding flags cannot be combined with a non-direct embedding mode")
		}
		p.AI.Embedding.Protocol, p.AI.Embedding.Endpoint, p.AI.Embedding.Model, p.AI.Embedding.Dimensions = "", "", "", 0
	}
	if cmd.Flags().Changed("rerank-mode") && p.AI.Rerank.Mode != auth.ServiceDirect {
		if cmd.Flags().Changed("rerank-protocol") || cmd.Flags().Changed("rerank-endpoint") || cmd.Flags().Changed("rerank-model") || cmd.Flags().Changed("rerank-dimensions") {
			return p, errors.New("direct rerank flags cannot be combined with a non-direct rerank mode")
		}
		p.AI.Rerank.Protocol, p.AI.Rerank.Endpoint, p.AI.Rerank.Model, p.AI.Rerank.Dimensions = "", "", "", 0
	}
	if err := configureProviderONNXExecution(cmd, reader, "embedding", &p.AI.Embedding, o.embeddingDevice, o.embeddingDeviceID); err != nil {
		return p, err
	}
	if err := configureProviderONNXExecution(cmd, reader, "rerank", &p.AI.Rerank, o.rerankDevice, o.rerankDeviceID); err != nil {
		return p, err
	}
	if p.Type != auth.ProviderBroker {
		setString("s3-bucket", &p.S3.Bucket, o.s3Bucket)
		setString("s3-region", &p.S3.Region, o.s3Region)
		setString("s3-endpoint", &p.S3.Endpoint, o.s3Endpoint)
		setString("s3-prefix", &p.S3.Prefix, o.s3Prefix)
		setString("s3-credential-source", &p.S3.CredentialSource, o.s3CredentialSource)
	}
	stsChanged := cmd.Flags().Changed("sts-endpoint") || cmd.Flags().Changed("sts-role-arn") || cmd.Flags().Changed("sts-session-name") || cmd.Flags().Changed("sts-duration") || cmd.Flags().Changed("sts-use-access-token")
	if current == nil {
		stsChanged = o.stsRoleARN != "" || o.stsEndpoint != ""
	}
	if o.clearSTS && stsChanged {
		return p, errors.New("--clear-sts cannot be combined with STS configuration flags")
	}
	if o.clearSTS {
		p.STS = nil
	} else if stsChanged {
		if p.Type != auth.ProviderOIDC {
			return p, errors.New("STS web identity flags require an OIDC provider")
		}
		if p.STS == nil {
			p.STS = &auth.STSConfig{}
		}
		setString("sts-endpoint", &p.STS.Endpoint, o.stsEndpoint)
		setString("sts-role-arn", &p.STS.RoleARN, o.stsRoleARN)
		setString("sts-session-name", &p.STS.RoleSessionName, o.stsSessionName)
		if current == nil || cmd.Flags().Changed("sts-duration") {
			p.STS.DurationSeconds = o.stsDuration
		}
		if current == nil || cmd.Flags().Changed("sts-use-access-token") {
			p.STS.UseAccessToken = o.stsUseAccessToken
		}
	}
	if p.Type == auth.ProviderOIDC && p.STS != nil && p.S3.CredentialSource == "" {
		p.S3.CredentialSource = "sts"
	}
	return p, auth.ValidateProvider(p)
}

func configureProviderONNXExecution(cmd *cobra.Command, reader *bufio.Reader, serviceName string, service *auth.AIServiceConfig, deviceValue, deviceIDValue string) error {
	deviceFlag, deviceIDFlag := serviceName+"-device", serviceName+"-device-id"
	deviceSet, deviceIDSet := cmd.Flags().Changed(deviceFlag), cmd.Flags().Changed(deviceIDFlag)
	mode := service.Mode
	if mode == "" {
		mode = auth.ServiceLocal
	}
	if mode != auth.ServiceLocal {
		if deviceSet || deviceIDSet {
			return fmt.Errorf("--%s and --%s require --%s-mode=local", deviceFlag, deviceIDFlag, serviceName)
		}
		service.ONNX = nil
		return nil
	}

	current := auth.DefaultONNXExecutionConfig()
	if service.ONNX != nil {
		current = *service.ONNX
	}
	device, deviceID := string(current.Device), strconv.Itoa(current.DeviceID)
	if deviceSet {
		device = deviceValue
	} else if !nonInteractive {
		device = promptSimple(reader, serviceName+" ONNX device (auto/cpu/cuda/coreml)", device)
	}
	if deviceIDSet {
		deviceID = deviceIDValue
	} else if !nonInteractive {
		deviceID = promptSimple(reader, serviceName+" ONNX device ID", deviceID)
	}
	execution, err := ai.ParseONNXExecution(device, deviceID)
	if err != nil {
		return fmt.Errorf("invalid %s ONNX execution: %w", serviceName, err)
	}
	service.ONNX = &execution
	return nil
}

func inputRequiredValue(reader *bufio.Reader, label string) (string, error) {
	if nonInteractive {
		return "", fmt.Errorf("%s is required in non-interactive mode; pass the corresponding flag", label)
	}
	fmt.Fprintf(os.Stderr, "Enter %s: ", label)
	line, err := reader.ReadString('\n')
	if err != nil {
		return "", fmt.Errorf("read %s: %w", label, err)
	}
	value := strings.TrimSpace(line)
	if value == "" {
		return "", fmt.Errorf("%s is required", label)
	}
	return value, nil
}

func newProviderListCmd() *cobra.Command {
	return &cobra.Command{Use: "list", Short: "List configured providers", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, args []string) error {
		store, err := auth.Open()
		if err != nil {
			return err
		}
		state, err := store.Load()
		if err != nil {
			return err
		}
		p := output.NewPrinter("")
		rows := make([][2]string, 0, len(state.Providers))
		for _, name := range state.ProviderNames() {
			provider := state.Providers[name]
			rows = append(rows, [2]string{name, string(provider.Type) + " (revision " + strconv.FormatUint(provider.Revision, 10) + ")"})
		}
		if len(rows) == 0 {
			p.Info("No providers configured")
			return nil
		}
		p.Table([2]string{"PROVIDER", "TYPE"}, rows)
		return nil
	}}
}
func newProviderShowCmd() *cobra.Command {
	return &cobra.Command{Use: "show <name>", Short: "Show a provider with secrets redacted", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		store, err := auth.Open()
		if err != nil {
			return err
		}
		state, err := store.Load()
		if err != nil {
			return err
		}
		provider, ok := state.Providers[args[0]]
		if !ok {
			return fmt.Errorf("provider %q does not exist", args[0])
		}
		return printJSON(provider.Redacted())
	}}
}
func newProviderRemoveCmd() *cobra.Command {
	var cascade, yes bool
	cmd := &cobra.Command{Use: "remove <name>", Short: "Remove a provider", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		if !yes {
			if nonInteractive {
				return errors.New("--yes is required to remove a provider in non-interactive mode")
			}
			ok, err := confirm("Remove provider " + args[0] + "?")
			if err != nil {
				return err
			}
			if !ok {
				return errors.New("provider removal cancelled")
			}
		}
		store, err := auth.Open()
		if err != nil {
			return err
		}
		if err := store.RemoveProvider(args[0], cascade); err != nil {
			return err
		}
		output.NewPrinter("").Success("Provider %s removed", args[0])
		return nil
	}}
	cmd.Flags().BoolVar(&cascade, "cascade", false, "Also remove profiles that reference this provider")
	cmd.Flags().BoolVar(&yes, "yes", false, "Confirm removal without prompting")
	return cmd
}

type loginOptions struct {
	profile, provider, username, organization, mcpKey, brokerKey, embeddingAPIKey, rerankAPIKey, s3AccessKey, s3SecretKey, s3SessionToken, awsProfile, accessToken, refreshToken, idToken, expiresAt string
	teams                                                                                                                                                                                            []string
	anonymous                                                                                                                                                                                        bool
}

func newLoginCmd() *cobra.Command {
	var o loginOptions
	cmd := &cobra.Command{Use: "login", Short: "Authenticate a profile and make it active", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, args []string) error { return runLogin(cmd, o) }}
	cmd.Flags().StringVar(&o.profile, "profile", "", "Profile name")
	cmd.Flags().StringVar(&o.provider, "provider", "", "Configured provider name")
	cmd.Flags().StringVar(&o.username, "username", "", "Local username")
	cmd.Flags().StringVar(&o.organization, "organization", "", "Local organization")
	cmd.Flags().StringSliceVar(&o.teams, "team", nil, "Local team (repeatable)")
	cmd.Flags().BoolVar(&o.anonymous, "anonymous", false, "Activate an anonymous profile without a broker credential")
	cmd.Flags().StringVar(&o.mcpKey, "mcp-key", "", "Static MCP bearer key for this profile")
	cmd.Flags().StringVar(&o.brokerKey, "broker-key", "", "Static broker bearer key for this profile")
	cmd.Flags().StringVar(&o.embeddingAPIKey, "embedding-api-key", "", "Direct embedding provider API key")
	cmd.Flags().StringVar(&o.rerankAPIKey, "rerank-api-key", "", "Direct rerank provider API key")
	cmd.Flags().StringVar(&o.s3AccessKey, "s3-access-key", "", "S3 access key ID for a local provider")
	cmd.Flags().StringVar(&o.s3SecretKey, "s3-secret-key", "", "S3 secret access key for a local provider")
	cmd.Flags().StringVar(&o.s3SessionToken, "s3-session-token", "", "S3 session token for a local provider")
	cmd.Flags().StringVar(&o.awsProfile, "aws-profile", "", "AWS shared-config profile for a local provider")
	cmd.Flags().StringVar(&o.accessToken, "access-token", "", "OIDC access token for non-interactive login")
	cmd.Flags().StringVar(&o.refreshToken, "refresh-token", "", "OIDC refresh token")
	cmd.Flags().StringVar(&o.idToken, "id-token", "", "OIDC ID token for verified identity claims")
	cmd.Flags().StringVar(&o.expiresAt, "token-expires-at", "", "OIDC access token expiration (RFC3339)")
	return cmd
}

func runLogin(cmd *cobra.Command, o loginOptions) error {
	var err error
	if o.profile == "" {
		o.profile, err = inputValue("profile name", false)
		if err != nil {
			return err
		}
	}
	if o.provider == "" {
		o.provider, err = inputValue("provider name", false)
		if err != nil {
			return err
		}
	}
	store, err := auth.Open()
	if err != nil {
		return err
	}
	state, err := store.Load()
	if err != nil {
		return err
	}
	provider, ok := state.Providers[o.provider]
	if !ok {
		return fmt.Errorf("provider %q does not exist; configure it with `%s provider add`", o.provider, brand.BinName())
	}
	var profile auth.Profile
	switch provider.Type {
	case auth.ProviderLocal:
		if o.anonymous {
			if provider.Broker == nil || !provider.Broker.AllowAnonymous {
				return errors.New("--anonymous requires a provider configured with --broker-allow-anonymous")
			}
			if cmd.Flags().Changed("username") || cmd.Flags().Changed("organization") || cmd.Flags().Changed("team") || cmd.Flags().Changed("broker-key") {
				return errors.New("--anonymous cannot be combined with username, organization, team, or broker-key")
			}
			o.username = "anonymous"
		} else if o.username == "" {
			o.username, err = inputValue("username", false)
			if err != nil {
				return err
			}
		}
		if provider.S3.Bucket != "" && o.awsProfile == "" && (o.s3AccessKey == "" || o.s3SecretKey == "") && (provider.Local == nil || !provider.Local.AllowAWSCredentialChain) {
			if o.s3AccessKey == "" {
				o.s3AccessKey, err = inputValue("S3 access key", true)
				if err != nil {
					return err
				}
			}
			if o.s3SecretKey == "" {
				o.s3SecretKey, err = inputValue("S3 secret key", true)
				if err != nil {
					return err
				}
			}
		}
		if o.brokerKey == "" && provider.Broker != nil && !provider.Broker.AllowAnonymous {
			o.brokerKey, err = inputValue("broker key", true)
			if err != nil {
				return err
			}
		}
		if o.embeddingAPIKey == "" && provider.AI.Embedding.Mode == auth.ServiceDirect {
			o.embeddingAPIKey, err = inputValue("embedding API key", true)
			if err != nil {
				return err
			}
		}
		if o.rerankAPIKey == "" && provider.AI.Rerank.Mode == auth.ServiceDirect {
			o.rerankAPIKey, err = inputValue("rerank API key", true)
			if err != nil {
				return err
			}
		}
		issuer := "local:" + provider.Name
		if o.anonymous {
			issuer = "anonymous"
		}
		profile = auth.Profile{Name: o.profile, Provider: provider.Name, ProviderRevision: provider.Revision, Issuer: issuer, Subject: o.username, Username: o.username, Organization: o.organization, Teams: o.teams, MCPKey: o.mcpKey, BrokerKey: o.brokerKey, EmbeddingAPIKey: o.embeddingAPIKey, RerankAPIKey: o.rerankAPIKey, S3: auth.S3Credentials{AccessKeyID: o.s3AccessKey, SecretAccessKey: o.s3SecretKey, SessionToken: o.s3SessionToken, AWSProfile: o.awsProfile}}
	case auth.ProviderOIDC:
		if o.anonymous {
			return errors.New("--anonymous is supported only by local providers")
		}
		if o.brokerKey != "" || o.mcpKey != "" {
			return errors.New("OIDC providers use the OIDC access token; --broker-key and --mcp-key are supported only by local providers")
		}
		client := auth.NewOIDCClient()
		if nonInteractive {
			if o.accessToken == "" || o.idToken == "" {
				return errors.New("--access-token and --id-token are required for non-interactive OIDC login")
			}
			expiry := time.Now().Add(time.Hour)
			if o.expiresAt != "" {
				expiry, err = time.Parse(time.RFC3339, o.expiresAt)
				if err != nil {
					return fmt.Errorf("invalid --token-expires-at: %w", err)
				}
			}
			profile, err = client.LoginWithTokens(cmd.Context(), provider, o.accessToken, o.refreshToken, o.idToken, expiry)
		} else {
			ctx, cancel := context.WithTimeout(cmd.Context(), 5*time.Minute)
			defer cancel()
			profile, err = client.LoginInteractive(ctx, provider, func(target string) error {
				output.NewPrinter("").Info("Open this URL to authenticate:\n%s", target)
				openBrowser(target)
				return nil
			})
		}
		if err != nil {
			return err
		}
		profile.Name = o.profile
		profile.BrokerKey = o.brokerKey
		profile.EmbeddingAPIKey = o.embeddingAPIKey
		profile.RerankAPIKey = o.rerankAPIKey
	case auth.ProviderBroker:
		if nonInteractive {
			return errors.New("broker login requires the interactive browser flow")
		}
		if o.anonymous || o.username != "" || o.organization != "" || len(o.teams) > 0 || o.mcpKey != "" || o.brokerKey != "" || o.idToken != "" || o.accessToken != "" || o.refreshToken != "" || o.expiresAt != "" {
			return errors.New("broker login obtains identity and tokens from the broker page; local identity, static-key, and token flags are not accepted")
		}
		ctx, cancel := context.WithTimeout(cmd.Context(), 5*time.Minute)
		defer cancel()
		resolved, resolveErr := auth.BrokerOIDCProvider(ctx, provider, nil)
		if resolveErr != nil {
			return resolveErr
		}
		profile, err = auth.NewOIDCClient().LoginInteractive(ctx, resolved, func(target string) error {
			output.NewPrinter("").Info("Open this Broker URL to authenticate:\n%s", target)
			openBrowser(target)
			return nil
		})
		if err != nil {
			return err
		}
		profile.Name = o.profile
	default:
		return fmt.Errorf("unsupported provider type %q", provider.Type)
	}
	if profile.EmbeddingAPIKey == "" && provider.AI.Embedding.Mode == auth.ServiceDirect {
		profile.EmbeddingAPIKey, err = inputValue("embedding API key", true)
		if err != nil {
			return err
		}
	}
	if profile.RerankAPIKey == "" && provider.AI.Rerank.Mode == auth.ServiceDirect {
		profile.RerankAPIKey, err = inputValue("rerank API key", true)
		if err != nil {
			return err
		}
	}
	if provider.STS != nil {
		profile.S3, err = (auth.AWSSTSExchanger{}).Exchange(cmd.Context(), provider, profile)
		if err != nil {
			return err
		}
	}
	if provider.Type == auth.ProviderBroker {
		if err = populateBrokerStorageForLogin(cmd.Context(), provider, &profile, nil); err != nil {
			return err
		}
	}
	if err := store.Login(profile); err != nil {
		return err
	}
	output.NewPrinter("").Success("Logged in and activated profile %s", profile.Name)
	return nil
}

func populateBrokerStorageForLogin(ctx context.Context, provider auth.Provider, profile *auth.Profile, client *http.Client) error {
	discovery, err := auth.DiscoverBroker(ctx, provider, client)
	if err != nil {
		return err
	}
	profile.S3 = auth.S3Credentials{}
	profile.BrokerS3Disabled = discovery.Services.S3Credentials == nil
	return nil
}

func cloneStringMap(values map[string]string) map[string]string {
	if len(values) == 0 {
		return nil
	}
	cloned := make(map[string]string, len(values))
	for key, value := range values {
		cloned[strings.TrimSpace(key)] = strings.TrimSpace(value)
	}
	return cloned
}

func newLogoutCmd() *cobra.Command {
	var profile string
	cmd := &cobra.Command{Use: "logout", Short: "Remove credentials and the selected account profile", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, args []string) error {
		store, err := auth.Open()
		if err != nil {
			return err
		}
		if err := store.Logout(profile); err != nil {
			return err
		}
		output.NewPrinter("").Success("Logged out%s", func() string {
			if profile != "" {
				return " profile " + profile
			}
			return " active profile"
		}())
		return nil
	}}
	cmd.Flags().StringVar(&profile, "profile", "", "Profile to log out (defaults to active)")
	return cmd
}
func newAccountCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "account", Short: "Inspect and select account profiles", RunE: func(cmd *cobra.Command, args []string) error {
		if len(args) > 0 {
			return cobra.NoArgs(cmd, args)
		}
		store, err := auth.Open()
		if err != nil {
			return err
		}
		snapshot, err := store.Active()
		if err != nil {
			return err
		}
		return printJSON(snapshot.Profile.Redacted())
	}}
	cmd.AddCommand(newAccountListCmd(), newAccountShowCmd(), newAccountUseCmd())
	return cmd
}
func newAccountListCmd() *cobra.Command {
	return &cobra.Command{Use: "list", Short: "List account profiles", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, args []string) error {
		store, err := auth.Open()
		if err != nil {
			return err
		}
		state, err := store.Load()
		if err != nil {
			return err
		}
		rows := make([][2]string, 0, len(state.Profiles))
		for _, name := range state.ProfileNames() {
			mark := " "
			if name == state.ActiveProfile {
				mark = "*"
			}
			rows = append(rows, [2]string{mark + " " + name, state.Profiles[name].Provider})
		}
		if len(rows) == 0 {
			output.NewPrinter("").Info("No account profiles; run `%s login`", brand.BinName())
			return nil
		}
		output.NewPrinter("").Table([2]string{"PROFILE", "PROVIDER"}, rows)
		return nil
	}}
}
func newAccountShowCmd() *cobra.Command {
	return &cobra.Command{Use: "show [profile]", Short: "Show a profile with secrets redacted", Args: cobra.MaximumNArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		store, err := auth.Open()
		if err != nil {
			return err
		}
		state, err := store.Load()
		if err != nil {
			return err
		}
		name := state.ActiveProfile
		if len(args) == 1 {
			name = args[0]
		}
		profile, ok := state.Profiles[name]
		if !ok {
			return fmt.Errorf("profile %q does not exist", name)
		}
		return printJSON(profile.Redacted())
	}}
}
func newAccountUseCmd() *cobra.Command {
	return &cobra.Command{Use: "use <profile>", Short: "Atomically activate a profile", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		store, err := auth.Open()
		if err != nil {
			return err
		}
		if err := store.UseProfile(args[0]); err != nil {
			return err
		}
		output.NewPrinter("").Success("Activated profile %s", args[0])
		return nil
	}}
}

func inputValue(label string, secret bool) (string, error) {
	if nonInteractive {
		return "", fmt.Errorf("%s is required in non-interactive mode; pass the corresponding flag", label)
	}
	fmt.Fprintf(os.Stderr, "Enter %s: ", label)
	var raw []byte
	var err error
	if secret && term.IsTerminal(int(os.Stdin.Fd())) {
		raw, err = term.ReadPassword(int(os.Stdin.Fd()))
		fmt.Fprintln(os.Stderr)
	} else {
		reader := bufio.NewReader(os.Stdin)
		var line string
		line, err = reader.ReadString('\n')
		raw = []byte(line)
	}
	if err != nil {
		return "", fmt.Errorf("read %s: %w", label, err)
	}
	value := strings.TrimSpace(string(raw))
	if value == "" {
		return "", fmt.Errorf("%s is required", label)
	}
	return value, nil
}
func confirm(question string) (bool, error) {
	value, err := inputValue(question+" [y/N]", false)
	if err != nil {
		return false, err
	}
	return strings.EqualFold(value, "y") || strings.EqualFold(value, "yes"), nil
}
func splitCSV(value string) []string {
	var out []string
	for _, item := range strings.Split(value, ",") {
		if item = strings.TrimSpace(item); item != "" {
			out = append(out, item)
		}
	}
	return out
}
func printJSON(value any) error {
	data, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		return err
	}
	output.NewPrinter("").Data(string(data))
	return nil
}
