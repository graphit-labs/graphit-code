use super::*;
fn limits() -> ProjectionLimits {
    ProjectionLimits::new(4096, 8192, 64, 255, 64).unwrap()
}
fn record(endpoint: Option<&str>, prefix: &str) -> io::Result<RemoteNativeRecord> {
    RemoteNativeRecord::new(
        "explicit-provider",
        "native-bucket",
        "region-1",
        endpoint,
        prefix,
        [7; 32],
        limits(),
    )
}
#[test]
fn topology_is_explicit_bounded_and_data_only() {
    let value = record(Some("https://storage.example:9443"), "tenant/native").unwrap();
    assert_eq!(value.provider_name(), "explicit-provider");
    assert_eq!(value.bucket(), "native-bucket");
    assert_eq!(value.region(), "region-1");
    assert_eq!(value.endpoint(), Some("https://storage.example:9443"));
    assert_eq!(value.prefix(), "tenant/native");
    assert_eq!(value.root_digest(), &[7; 32]);
    let encoded = serde_json::to_vec(&value).unwrap();
    let restored: RawRemoteNativeRecord = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(restored.restore(limits()).unwrap(), value);
    assert!(record(None, "").is_ok());
    assert!(
        RemoteNativeRecord::new("", "native-bucket", "region-1", None, "", [0; 32], limits())
            .is_err()
    );
    for bucket in [
        "",
        "ab",
        "Bucket",
        "a..b",
        "a.-b",
        "127.0.0.1",
        "s3://bucket",
        "-abc",
    ] {
        assert!(
            RemoteNativeRecord::new("chosen", bucket, "region-1", None, "", [0; 32], limits())
                .is_err(),
            "{bucket}"
        );
    }
    for region in ["", "region/path", "region?query", "region\n"] {
        assert!(
            RemoteNativeRecord::new(
                "chosen",
                "native-bucket",
                region,
                None,
                "",
                [0; 32],
                limits()
            )
            .is_err()
        );
    }
    for endpoint in [
        "s3://bucket",
        "https://user:secret@host",
        "https://host?query",
        "https://host#fragment",
        "https://host/path",
        "https://host/x/..",
        "https://host\\path",
    ] {
        assert!(record(Some(endpoint), "").is_err(), "{endpoint}");
    }
    for prefix in [
        "/root",
        "root/",
        "a//b",
        ".",
        "../b",
        "a/../b",
        "a\\b",
        "s3://bucket",
        "a\n",
    ] {
        assert!(record(None, prefix).is_err(), "{prefix}");
    }
    assert!(record(None, "literal%2F/✓").is_ok());
    assert!(record(None, &"x".repeat(1014)).is_err());
    assert!(
        value
            .validate(ProjectionLimits::new(3, 8192, 64, 255, 64).unwrap())
            .is_err()
    );
}

#[test]
fn raw_record_rejects_unknown_missing_and_wrong_types() {
    let valid = serde_json::to_value(record(None, "").unwrap()).unwrap();
    for field in ["provider_name", "bucket", "region", "prefix", "root_digest"] {
        let mut bad = valid.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<RawRemoteNativeRecord>(bad).is_err());
        let mut bad = valid.clone();
        bad[field] = serde_json::Value::Null;
        assert!(serde_json::from_value::<RawRemoteNativeRecord>(bad).is_err());
    }
    let mut bad = valid.clone();
    bad["credential"] = "secret".into();
    assert!(serde_json::from_value::<RawRemoteNativeRecord>(bad).is_err());
    let mut bad = valid;
    bad["root_digest"] = serde_json::json!([1, 2]);
    assert!(serde_json::from_value::<RawRemoteNativeRecord>(bad).is_err());
}
