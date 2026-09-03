package config

import "strings"

// S3Config is the resolved object-store location and optional explicit credentials of the Hub.
type S3Config struct {
	Bucket          string
	Region          string
	Endpoint        string
	Prefix          string
	AccessKeyID     string
	SecretAccessKey string
}

func (c S3Config) Configured() bool { return c.Bucket != "" }

func (c S3Config) HasStaticCredentials() bool {
	return c.AccessKeyID != "" && c.SecretAccessKey != ""
}

func ResolveHubBucket(inlineCfg, projectCfg ConfigMap) string {
	return ResolveConfig("hub.bucket", inlineCfg, projectCfg)
}

func ResolveHubRegion(inlineCfg, projectCfg ConfigMap) string {
	return ResolveConfig("hub.region", inlineCfg, projectCfg)
}

func ResolveHubEndpoint(inlineCfg, projectCfg ConfigMap) string {
	return ResolveConfig("hub.endpoint", inlineCfg, projectCfg)
}

func ResolveHubPrefix(inlineCfg, projectCfg ConfigMap) string {
	return normalizePrefix(ResolveConfig("hub.prefix", inlineCfg, projectCfg))
}

func ResolveHubAccessKeyID(inlineCfg, projectCfg ConfigMap) string {
	return strings.TrimSpace(ResolveConfig("hub.access_key_id", inlineCfg, projectCfg))
}

func ResolveHubSecretAccessKey(inlineCfg, projectCfg ConfigMap) string {
	return ResolveConfig("hub.secret_access_key", inlineCfg, projectCfg)
}

func HubBucket() string { return ResolveHubBucket(nil, nil) }

func HubRegion() string { return ResolveHubRegion(nil, nil) }

func HubEndpoint() string { return ResolveHubEndpoint(nil, nil) }

func HubPrefix() string { return ResolveHubPrefix(nil, nil) }

func ResolveHubS3(inlineCfg, projectCfg ConfigMap) S3Config {
	return S3Config{
		Bucket:          ResolveHubBucket(inlineCfg, projectCfg),
		Region:          ResolveHubRegion(inlineCfg, projectCfg),
		Endpoint:        ResolveHubEndpoint(inlineCfg, projectCfg),
		Prefix:          ResolveHubPrefix(inlineCfg, projectCfg),
		AccessKeyID:     ResolveHubAccessKeyID(inlineCfg, projectCfg),
		SecretAccessKey: ResolveHubSecretAccessKey(inlineCfg, projectCfg),
	}
}

func HubS3Config() S3Config { return ResolveHubS3(nil, nil) }

func SetGlobalS3Credentials(accessKeyID, secretAccessKey string) error {
	cfg, err := LoadGlobalConfig()
	if err != nil {
		return err
	}
	if accessKeyID == "" || secretAccessKey == "" {
		UnsetConfigValue(cfg, "hub.access_key_id")
		UnsetConfigValue(cfg, "hub.secret_access_key")
	} else {
		SetConfigValue(cfg, "hub.access_key_id", accessKeyID)
		SetConfigValue(cfg, "hub.secret_access_key", secretAccessKey)
	}
	return SaveGlobalConfig(cfg)
}

func normalizePrefix(prefix string) string {
	return strings.Trim(strings.TrimSpace(prefix), "/")
}
