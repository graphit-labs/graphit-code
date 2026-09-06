package s3store

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"strings"

	awsconfig "github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/credentials"
	"github.com/aws/aws-sdk-go-v2/service/s3"
	"github.com/aws/aws-sdk-go-v2/service/s3/types"

	"github.com/graphit-labs/graphit-code/internal/config"
)

// ErrNotConfigured is returned when no bucket has been configured, which is the local-only
// mode: every caller treats it as "there is no remote", never as a failure.
var ErrNotConfigured = errors.New("no hub bucket configured")

// ErrNotFound distinguishes a missing object from a transport or permission failure, which
// the callers need because a missing registry is a first run and a denied one is a setup bug.
var ErrNotFound = errors.New("object not found")

// ErrPreconditionFailed reports a failed compare-and-swap or put-if-absent operation.
var ErrPreconditionFailed = errors.New("object precondition failed")

type Store struct {
	client   *s3.Client
	bucket   string
	prefix   string
	endpoint string
}

type Object struct {
	Key  string
	Size int64
	ETag string
}

type Value struct {
	Data []byte
	ETag string
}

type Page struct {
	Objects    []Object
	NextCursor string
}

// New builds a store from the resolved Hub configuration.
func New(ctx context.Context, cfg config.S3Config) (*Store, error) {
	if !cfg.Configured() {
		return nil, ErrNotConfigured
	}

	opts := []func(*awsconfig.LoadOptions) error{}
	if cfg.Region != "" {
		opts = append(opts, awsconfig.WithRegion(cfg.Region))
	}
	if cfg.HasStaticCredentials() {
		opts = append(opts, awsconfig.WithCredentialsProvider(
			credentials.NewStaticCredentialsProvider(cfg.AccessKeyID, cfg.SecretAccessKey, ""),
		))
	}
	awsCfg, err := awsconfig.LoadDefaultConfig(ctx, opts...)
	if err != nil {
		return nil, fmt.Errorf("loading aws configuration: %w", err)
	}

	client := s3.NewFromConfig(awsCfg, func(o *s3.Options) {
		if cfg.Endpoint != "" {
			o.BaseEndpoint = &cfg.Endpoint
			// NOTE: MinIO and most S3-compatible servers do not serve virtual-host style
			// buckets, so a custom endpoint implies path style.
			o.UsePathStyle = true
		}
	})

	return &Store{client: client, bucket: cfg.Bucket, prefix: cfg.Prefix, endpoint: cfg.Endpoint}, nil
}

func (s *Store) Bucket() string { return s.bucket }

func (s *Store) Prefix() string { return s.prefix }

// Key qualifies a relative path with the configured prefix.
func (s *Store) Key(relPath string) string { return JoinKey(s.prefix, relPath) }

// URI is the storage location the graph and search engines mount directly.
func (s *Store) URI(relPath string) string { return URI(s.bucket, s.Key(relPath)) }

// EnsureBucket verifies the bucket exists and is reachable with the resolved credentials.
// This is what turns a missing credential into an error at setup instead of a confusing
// failure on the first query.
func (s *Store) EnsureBucket(ctx context.Context) error {
	_, err := s.client.HeadBucket(ctx, &s3.HeadBucketInput{Bucket: &s.bucket})
	if err != nil {
		return fmt.Errorf("reaching bucket %q: %w", s.bucket, err)
	}
	return nil
}

func (s *Store) Get(ctx context.Context, relPath string) ([]byte, error) {
	value, err := s.GetValue(ctx, relPath)
	if err != nil {
		return nil, err
	}
	return value.Data, nil
}

func (s *Store) GetValue(ctx context.Context, relPath string) (Value, error) {
	key := s.Key(relPath)
	out, err := s.client.GetObject(ctx, &s3.GetObjectInput{Bucket: &s.bucket, Key: &key})
	if err != nil {
		if isMissing(err) {
			return Value{}, fmt.Errorf("%s: %w", key, ErrNotFound)
		}
		return Value{}, fmt.Errorf("get %s: %w", key, err)
	}
	defer out.Body.Close()

	data, err := io.ReadAll(out.Body)
	if err != nil {
		return Value{}, fmt.Errorf("reading %s: %w", key, err)
	}
	etag := ""
	if out.ETag != nil {
		etag = *out.ETag
	}
	return Value{Data: data, ETag: etag}, nil
}

func (s *Store) Put(ctx context.Context, relPath string, data []byte) error {
	_, err := s.put(ctx, relPath, data, "", "")
	return err
}

func (s *Store) PutIfAbsent(ctx context.Context, relPath string, data []byte) (string, error) {
	return s.put(ctx, relPath, data, "", "*")
}

func (s *Store) PutIfMatch(ctx context.Context, relPath string, data []byte, etag string) (string, error) {
	if strings.TrimSpace(etag) == "" {
		return "", fmt.Errorf("put %s: empty ETag: %w", s.Key(relPath), ErrPreconditionFailed)
	}
	return s.put(ctx, relPath, data, etag, "")
}

func (s *Store) put(ctx context.Context, relPath string, data []byte, ifMatch, ifNoneMatch string) (string, error) {
	key := s.Key(relPath)
	in := &s3.PutObjectInput{
		Bucket: &s.bucket,
		Key:    &key,
		Body:   bytes.NewReader(data),
	}
	if ifMatch != "" {
		in.IfMatch = &ifMatch
	}
	if ifNoneMatch != "" {
		in.IfNoneMatch = &ifNoneMatch
	}
	out, err := s.client.PutObject(ctx, in)
	if err != nil {
		if isPreconditionFailed(err) {
			return "", fmt.Errorf("put %s: %w", key, ErrPreconditionFailed)
		}
		return "", fmt.Errorf("put %s: %w", key, err)
	}
	if out.ETag == nil {
		return "", nil
	}
	return *out.ETag, nil
}

func (s *Store) Delete(ctx context.Context, relPath string) error {
	return s.delete(ctx, relPath, "")
}

func (s *Store) DeleteIfMatch(ctx context.Context, relPath, etag string) error {
	if strings.TrimSpace(etag) == "" {
		return fmt.Errorf("delete %s: empty ETag: %w", s.Key(relPath), ErrPreconditionFailed)
	}
	return s.delete(ctx, relPath, etag)
}

func (s *Store) delete(ctx context.Context, relPath, ifMatch string) error {
	key := s.Key(relPath)
	in := &s3.DeleteObjectInput{Bucket: &s.bucket, Key: &key}
	if ifMatch != "" {
		in.IfMatch = &ifMatch
	}
	if _, err := s.client.DeleteObject(ctx, in); err != nil {
		if isPreconditionFailed(err) {
			return fmt.Errorf("delete %s: %w", key, ErrPreconditionFailed)
		}
		return fmt.Errorf("delete %s: %w", key, err)
	}
	return nil
}

func (s *Store) Exists(ctx context.Context, relPath string) (bool, error) {
	key := s.Key(relPath)
	_, err := s.client.HeadObject(ctx, &s3.HeadObjectInput{Bucket: &s.bucket, Key: &key})
	if err != nil {
		if isMissing(err) {
			return false, nil
		}
		return false, fmt.Errorf("head %s: %w", key, err)
	}
	return true, nil
}

// List walks every object under relPrefix, following continuation tokens. Keys come back
// relative to the configured prefix so callers never see it.
func (s *Store) List(ctx context.Context, relPrefix string) ([]Object, error) {
	full := s.Key(relPrefix)
	search := full
	if search != "" {
		search += "/"
	}

	var out []Object
	var token *string
	for {
		page, err := s.client.ListObjectsV2(ctx, &s3.ListObjectsV2Input{
			Bucket:            &s.bucket,
			Prefix:            &search,
			ContinuationToken: token,
		})
		if err != nil {
			return nil, fmt.Errorf("list %s: %w", search, err)
		}
		for _, obj := range page.Contents {
			if obj.Key == nil {
				continue
			}
			rel := strings.TrimPrefix(*obj.Key, s.prefixWithSlash())
			var size int64
			if obj.Size != nil {
				size = *obj.Size
			}
			etag := ""
			if obj.ETag != nil {
				etag = *obj.ETag
			}
			out = append(out, Object{Key: rel, Size: size, ETag: etag})
		}
		if page.IsTruncated == nil || !*page.IsTruncated {
			return out, nil
		}
		token = page.NextContinuationToken
	}
}

// ListPage lists keys beginning with relPrefix without materializing every matching object.
// The returned cursor is opaque to callers and can be passed to the next call unchanged.
func (s *Store) ListPage(ctx context.Context, relPrefix string, limit int, cursor string) (Page, error) {
	if limit <= 0 || limit > 1000 {
		limit = 100
	}
	full := s.Key(relPrefix)
	maxKeys := int32(limit)
	in := &s3.ListObjectsV2Input{Bucket: &s.bucket, Prefix: &full, MaxKeys: &maxKeys}
	if cursor != "" {
		in.ContinuationToken = &cursor
	}
	out, err := s.client.ListObjectsV2(ctx, in)
	if err != nil {
		return Page{}, fmt.Errorf("list %s: %w", full, err)
	}
	page := Page{Objects: make([]Object, 0, len(out.Contents))}
	for _, obj := range out.Contents {
		if obj.Key == nil {
			continue
		}
		rel := strings.TrimPrefix(*obj.Key, s.prefixWithSlash())
		var size int64
		if obj.Size != nil {
			size = *obj.Size
		}
		etag := ""
		if obj.ETag != nil {
			etag = *obj.ETag
		}
		page.Objects = append(page.Objects, Object{Key: rel, Size: size, ETag: etag})
	}
	if out.IsTruncated != nil && *out.IsTruncated && out.NextContinuationToken != nil {
		page.NextCursor = *out.NextContinuationToken
	}
	return page, nil
}

// DeletePrefix removes everything under relPrefix. This is how a published version is
// retracted: the unit of an artifact is a prefix, not a file.
func (s *Store) DeletePrefix(ctx context.Context, relPrefix string) error {
	objs, err := s.List(ctx, relPrefix)
	if err != nil {
		return err
	}
	return s.deleteObjects(ctx, relPrefix, objs)
}

func (s *Store) deleteObjects(ctx context.Context, operation string, objs []Object) error {
	if len(objs) == 0 {
		return nil
	}

	const batch = 1000
	for start := 0; start < len(objs); start += batch {
		end := min(start+batch, len(objs))
		ids := make([]types.ObjectIdentifier, 0, end-start)
		for _, o := range objs[start:end] {
			key := s.Key(o.Key)
			ids = append(ids, types.ObjectIdentifier{Key: &key})
		}
		if _, err := s.client.DeleteObjects(ctx, &s3.DeleteObjectsInput{
			Bucket: &s.bucket,
			Delete: &types.Delete{Objects: ids},
		}); err != nil {
			return fmt.Errorf("delete objects for %s: %w", operation, err)
		}
	}
	return nil
}

// UploadDir mirrors a local directory into relPrefix, preserving relative paths with forward
// slashes so the keys read the same on every platform.
func (s *Store) UploadDir(ctx context.Context, localDir, relPrefix string) error {
	wanted := make(map[string]struct{})
	if err := filepath.WalkDir(localDir, func(path string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		rel, relErr := filepath.Rel(localDir, path)
		if relErr != nil {
			return relErr
		}
		data, readErr := os.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		key := JoinKey(relPrefix, filepath.ToSlash(rel))
		if err := s.Put(ctx, key, data); err != nil {
			return err
		}
		wanted[key] = struct{}{}
		return nil
	}); err != nil {
		return err
	}

	remote, err := s.List(ctx, relPrefix)
	if err != nil {
		return err
	}
	stale := make([]Object, 0)
	for _, obj := range remote {
		if _, ok := wanted[obj.Key]; !ok {
			stale = append(stale, obj)
		}
	}
	return s.deleteObjects(ctx, "mirror "+relPrefix, stale)
}

// DownloadPrefix materialises a remote prefix locally. Nothing in the query path uses it —
// both engines read s3:// directly — but publishing and diagnostics do.
func (s *Store) DownloadPrefix(ctx context.Context, relPrefix, localDir string) error {
	objs, err := s.List(ctx, relPrefix)
	if err != nil {
		return err
	}
	trim := strings.Trim(relPrefix, "/")
	for _, obj := range objs {
		data, getErr := s.Get(ctx, obj.Key)
		if getErr != nil {
			return getErr
		}
		rel := strings.TrimPrefix(strings.TrimPrefix(obj.Key, trim), "/")
		dest := filepath.Join(localDir, filepath.FromSlash(rel))
		if mkErr := os.MkdirAll(filepath.Dir(dest), 0o755); mkErr != nil {
			return mkErr
		}
		if wErr := os.WriteFile(dest, data, 0o644); wErr != nil {
			return wErr
		}
	}
	return nil
}

func (s *Store) prefixWithSlash() string {
	if s.prefix == "" {
		return ""
	}
	return s.prefix + "/"
}

func isMissing(err error) bool {
	var notFound *types.NotFound
	if errors.As(err, &notFound) {
		return true
	}
	var noSuchKey *types.NoSuchKey
	if errors.As(err, &noSuchKey) {
		return true
	}
	var apiErr interface{ ErrorCode() string }
	if errors.As(err, &apiErr) {
		switch apiErr.ErrorCode() {
		case "NotFound", "NoSuchKey", "404":
			return true
		}
	}
	return false
}

func isPreconditionFailed(err error) bool {
	var apiErr interface{ ErrorCode() string }
	if errors.As(err, &apiErr) {
		switch apiErr.ErrorCode() {
		case "PreconditionFailed", "ConditionalRequestConflict", "409", "412":
			return true
		}
	}
	return false
}
