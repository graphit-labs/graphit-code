use graphit_core::structured_output::{EncodedDocument, Limits, OutputError, OutputFormat, encode};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

const FORMATS: [OutputFormat; 2] = [OutputFormat::Json, OutputFormat::Toon];

fn limits() -> Limits {
    Limits {
        max_depth: 64,
        max_nodes: 100_000,
        max_input_bytes: 8 * 1024 * 1024,
        max_output_bytes: 8 * 1024 * 1024,
    }
}

fn decode(document: &EncodedDocument) -> Value {
    match document.format() {
        OutputFormat::Json => serde_json::from_str(document.as_str()).unwrap(),
        OutputFormat::Toon => toon_format::decode_default(document.as_str()).unwrap(),
    }
}

// TOON normalizes integral floats and negative zero. Preserve integer precision
// whenever both operands are integers; only compare as f64 for float inputs.
fn assert_data(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            if let Some(b) = b.as_i64() {
                assert_eq!(a.as_i64(), Some(b));
            } else if let Some(b) = b.as_u64() {
                assert_eq!(a.as_u64(), Some(b));
            } else {
                assert_eq!(a.as_f64(), b.as_f64());
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                assert_data(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (key, value) in b {
                assert_data(a.get(key).unwrap(), value);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}

#[test]
fn integer_boundaries_stay_exact_for_typed_consumers() {
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Integers {
        signed: i64,
        unsigned: u64,
    }
    let expected = Integers {
        signed: i64::MIN,
        unsigned: u64::MAX,
    };
    let value = serde_json::to_value(&expected).unwrap();
    for format in FORMATS {
        let document = encode(&value, format, limits()).unwrap();
        let actual: Integers = match format {
            OutputFormat::Json => serde_json::from_str(document.as_str()).unwrap(),
            OutputFormat::Toon => toon_format::decode_default(document.as_str()).unwrap(),
        };
        assert_eq!(actual, expected);
    }
}

#[test]
fn complete_json_values_preserve_presence_order_unicode_and_numbers() {
    let cases = vec![
        json!(null),
        json!(false),
        json!(true),
        json!(0),
        json!(-0.0),
        json!(""),
        json!([]),
        json!({}),
        json!("comma,pipe|colon:quote\"slash\\newline\nreturn\rtab\t雪🙂\u{feff}"),
        json!("\u{feff}BOM starts the string"),
        json!({"false": false, "zero": 0, "null": null, "string": "", "array": [], "object": {}}),
        json!([{}, [], null, false, 0, "", {"id": 1}, {"id": 2, "extra": []}]),
        json!({"results": [{"id": 1, "active": false}, {"id": 2, "active": true}], "next_cursor": null}),
        json!({"keyed": {"a": {"id": 1, "amount": 1.23456789}, "b": {"id": 2, "amount": std::f64::consts::E}}}),
        json!({"":{"": "empty keys"}, "commas,pipes|colons:": {"\n\r\t\"\\雪": "value"}}),
        json!([
            i64::MIN,
            i64::MAX,
            u64::MAX,
            std::f64::consts::PI,
            1.0000000000000002,
            0.8455124082255701,
            1e20,
            1e300,
            f64::MIN_POSITIVE,
            f64::from_bits(1)
        ]),
    ];
    for value in cases {
        let unchanged = value.clone();
        for format in FORMATS {
            let document = encode(&value, format, limits()).unwrap();
            assert_eq!(document.format(), format);
            assert_eq!(document.len(), document.as_str().len());
            assert_eq!(document.is_empty(), document.as_str().is_empty());
            assert_data(&decode(&document), &value);
            assert_eq!(value, unchanged);
        }
    }
}

#[test]
fn fractional_precision_is_exact_for_typed_consumers_without_two_decimal_rounding() {
    #[derive(Serialize, Deserialize)]
    struct Numbers {
        values: Vec<f64>,
    }
    let numbers = Numbers {
        values: vec![
            std::f64::consts::PI,
            1.0000000000000002,
            0.8455124082255701,
            1.23456789,
            1e-200,
            1e200,
            f64::from_bits(1),
            12.0,
            -0.0,
        ],
    };
    let input = serde_json::to_value(&numbers).unwrap();
    for format in FORMATS {
        let document = encode(&input, format, limits()).unwrap();
        let decoded: Numbers = match format {
            OutputFormat::Json => serde_json::from_str(document.as_str()).unwrap(),
            OutputFormat::Toon => toon_format::decode_default(document.as_str()).unwrap(),
        };
        for (actual, expected) in decoded.values.iter().zip(&numbers.values) {
            if *expected == 0.0 && format == OutputFormat::Toon {
                assert_eq!(*actual, 0.0);
            } else {
                assert_eq!(actual.to_bits(), expected.to_bits());
            }
        }
    }
}

fn ordered_object(reverse: bool) -> Value {
    let mut entries = vec![
        ("雪", json!([3, 1, 2])),
        ("z", json!({"b": false, "a": 0})),
        ("a", json!([{ "z": null, "a": "" }, {"b": [], "a": {}}])),
        ("é", json!("unicode")),
    ];
    if reverse {
        entries.reverse();
    }
    let mut map = Map::new();
    for (key, value) in entries {
        let value = match value {
            Value::Object(v) if reverse => Value::Object(v.into_iter().rev().collect()),
            other => other,
        };
        map.insert(key.to_owned(), value);
    }
    Value::Object(map)
}

#[test]
fn canonical_objects_ignore_insertion_order_without_reordering_arrays() {
    let a = ordered_object(false);
    let b = ordered_object(true);
    // Feature unification preserves input insertion order; the codec must still
    // provide its own canonical order, including nested objects.
    assert_ne!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
    for format in FORMATS {
        let a_doc = encode(&a, format, limits()).unwrap();
        let b_doc = encode(&b, format, limits()).unwrap();
        assert_eq!(a_doc.as_str(), b_doc.as_str());
        assert_eq!(decode(&a_doc)["雪"], json!([3, 1, 2]));
    }
    assert_eq!(
        encode(
            &json!({"z": 1, "a": {"z": 2, "a": 3}}),
            OutputFormat::Json,
            limits()
        )
        .unwrap()
        .as_str(),
        "{\"a\":{\"a\":3,\"z\":2},\"z\":1}"
    );
}

#[test]
fn public_dto_redaction_and_diagnostics_never_publish_a_skipped_secret() {
    #[derive(Serialize)]
    struct PublicDto<'a> {
        id: u32,
        active: bool,
        #[serde(skip_serializing)]
        secret: &'a str,
    }
    let canary = "PRIVATE-CODEC-CANARY";
    let dto = PublicDto {
        id: 7,
        active: false,
        secret: canary,
    };
    assert_eq!(dto.secret, canary);
    let public = serde_json::to_value(&dto).unwrap();
    for format in FORMATS {
        let document = encode(&public, format, limits()).unwrap();
        assert!(!document.as_str().contains(canary));
        assert!(!format!("{document:?}").contains(canary));
        let private = json!({canary: canary});
        for budget in [
            Limits {
                max_input_bytes: 0,
                ..limits()
            },
            Limits {
                max_output_bytes: 0,
                ..limits()
            },
        ] {
            let error = encode(&private, format, budget).unwrap_err();
            assert!(!format!("{error:?}: {error}").contains(canary));
        }
        // Redaction is a caller contract; ordinary authorized values remain data.
        assert_data(
            &decode(&encode(&private, format, limits()).unwrap()),
            &private,
        );
    }
}

#[test]
fn exact_input_and_output_byte_limits_include_escaped_keys_and_utf8() {
    let input = json!({"雪\n\"": [false, 0, null, "\u{feff}🙂", {}]});
    let size = serde_json::to_vec(&input).unwrap().len();
    for format in FORMATS {
        let baseline = encode(&input, format, limits()).unwrap();
        let exact = Limits {
            max_input_bytes: size,
            max_output_bytes: baseline.len(),
            ..limits()
        };
        assert_eq!(
            encode(&input, format, exact).unwrap().as_str(),
            baseline.as_str()
        );
        assert_eq!(
            encode(
                &input,
                format,
                Limits {
                    max_input_bytes: size - 1,
                    ..exact
                }
            )
            .unwrap_err(),
            OutputError::InputBytes
        );
        assert_eq!(
            encode(
                &input,
                format,
                Limits {
                    max_output_bytes: baseline.len() - 1,
                    ..exact
                }
            )
            .unwrap_err(),
            OutputError::OutputBytes
        );
        assert_eq!(
            encode(
                &input,
                format,
                Limits {
                    max_input_bytes: 0,
                    ..exact
                }
            )
            .unwrap_err(),
            OutputError::InputBytes
        );
        assert_eq!(
            encode(
                &input,
                format,
                Limits {
                    max_output_bytes: 0,
                    ..exact
                }
            )
            .unwrap_err(),
            OutputError::OutputBytes
        );
    }
    // The standard TOON encoding of a root empty object is a zero-byte document.
    let empty = encode(
        &json!({}),
        OutputFormat::Toon,
        Limits {
            max_input_bytes: 2,
            max_output_bytes: 0,
            ..limits()
        },
    )
    .unwrap();
    assert!(empty.is_empty());
    assert_eq!(decode(&empty), json!({}));
    assert_eq!(
        encode(
            &json!({}),
            OutputFormat::Json,
            Limits {
                max_output_bytes: 0,
                ..limits()
            }
        )
        .unwrap_err(),
        OutputError::OutputBytes
    );
}

#[test]
fn shape_limits_reject_before_render_and_invalid_budgets_are_explicit() {
    let input = json!({"array": [null, {}]}); // root, array, null, object = four nodes, depth two.
    for format in FORMATS {
        let exact = Limits {
            max_depth: 2,
            max_nodes: 4,
            ..limits()
        };
        encode(&input, format, exact).unwrap();
        assert_eq!(
            encode(
                &input,
                format,
                Limits {
                    max_depth: 1,
                    ..exact
                }
            )
            .unwrap_err(),
            OutputError::Depth
        );
        assert_eq!(
            encode(
                &input,
                format,
                Limits {
                    max_nodes: 3,
                    ..exact
                }
            )
            .unwrap_err(),
            OutputError::Nodes
        );
        for invalid in [
            Limits {
                max_depth: 65,
                ..limits()
            },
            Limits {
                max_nodes: 0,
                ..limits()
            },
        ] {
            assert_eq!(
                encode(&json!(null), format, invalid).unwrap_err(),
                OutputError::InvalidLimits
            );
        }
        encode(
            &json!(false),
            format,
            Limits {
                max_depth: 0,
                max_nodes: 1,
                ..limits()
            },
        )
        .unwrap();
        assert_eq!(
            encode(
                &json!([false]),
                format,
                Limits {
                    max_depth: 0,
                    ..limits()
                }
            )
            .unwrap_err(),
            OutputError::Depth
        );
        let broad = Value::Array(vec![Value::Null; 100_000]);
        assert_eq!(
            encode(
                &broad,
                format,
                Limits {
                    max_nodes: 2,
                    max_input_bytes: 0,
                    ..limits()
                }
            )
            .unwrap_err(),
            OutputError::Nodes
        );
        let mut deep = Value::Null;
        for _ in 0..64 {
            deep = Value::Array(vec![deep]);
        }
        encode(&deep, format, limits()).unwrap();
        deep = Value::Array(vec![deep]);
        assert_eq!(
            encode(
                &deep,
                format,
                Limits {
                    max_input_bytes: 0,
                    ..limits()
                }
            )
            .unwrap_err(),
            OutputError::Depth
        );
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    revision: String,
    source: String,
    files: Vec<ManifestFile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    path: String,
    source: String,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureFile {
    version: String,
    category: String,
    description: String,
    tests: Vec<Fixture>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Fixture {
    name: String,
    input: Value,
    expected: String,
    #[serde(default)]
    options: FixtureOptions,
    spec_section: Option<String>,
    note: Option<String>,
    min_spec_version: Option<String>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixtureOptions {
    delimiter: Option<String>,
    indent_size: Option<usize>,
}

#[test]
fn all_pinned_official_encode_fixtures_match_upstream_wire_and_canonical_data() {
    use sha2::{Digest, Sha256};
    use toon_format::{DecodeOptions, Delimiter, EncodeOptions, Indent};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/toon-v4.1");
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest.revision,
        "62f16b369408180f1faf1cba7da1b46d1f336f12"
    );
    assert_eq!(manifest.source, "https://github.com/toon-format/spec");
    assert_eq!(manifest.files.len(), 10);
    let mut cases = 0;
    let mut paths = std::collections::BTreeSet::new();
    for file in manifest.files {
        assert!(paths.insert(file.path.clone()), "duplicate fixture path");
        assert!(!file.path.contains('/') && !file.path.contains('\\'));
        let bytes = std::fs::read(root.join(&file.path)).unwrap();
        assert_eq!(
            Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            file.sha256
        );
        assert!(file.source.starts_with(&format!(
            "https://raw.githubusercontent.com/toon-format/spec/{}/",
            manifest.revision
        )));
        if file.path == "LICENSE" {
            assert!(String::from_utf8(bytes).unwrap().contains("MIT License"));
            continue;
        }
        let fixtures: FixtureFile = serde_json::from_slice(&bytes).unwrap();
        assert!(
            !fixtures.version.is_empty()
                && !fixtures.category.is_empty()
                && !fixtures.description.is_empty()
        );
        for fixture in fixtures.tests {
            let mut encode_options = EncodeOptions::new();
            let mut decode_options = DecodeOptions::new();
            if let Some(indent) = fixture.options.indent_size {
                encode_options = encode_options.with_indent(Indent::Spaces(indent));
                decode_options = decode_options.with_indent(Indent::Spaces(indent));
            }
            if let Some(delimiter) = fixture.options.delimiter {
                encode_options = encode_options.with_delimiter(match delimiter.as_str() {
                    "," => Delimiter::Comma,
                    "\t" => Delimiter::Tab,
                    "|" => Delimiter::Pipe,
                    _ => panic!("unknown official delimiter"),
                });
            }
            let actual = toon_format::encode(&fixture.input, &encode_options).unwrap();
            assert_eq!(actual, fixture.expected, "{}: {}", file.path, fixture.name);
            let expected_data: Value =
                toon_format::decode(&fixture.expected, &decode_options).unwrap();
            // Judge number normalization against the original JSON type: the
            // golden TOON decoder turns -0.0 into integer 0, while JSON keeps
            // its float. Integer inputs must still stay integers, exactly.
            assert_data(&expected_data, &fixture.input);
            for format in FORMATS {
                let document = encode(&fixture.input, format, limits()).unwrap();
                assert_data(&decode(&document), &fixture.input);
            }
            // Metadata is retained in vendored sources; it is not an execution option.
            let _ = (
                &fixture.spec_section,
                &fixture.note,
                &fixture.min_spec_version,
            );
            cases += 1;
        }
    }
    assert_eq!(cases, 179);
    assert_eq!(
        paths,
        [
            "LICENSE",
            "arrays-nested.json",
            "arrays-objects.json",
            "arrays-primitive.json",
            "arrays-tabular.json",
            "delimiters.json",
            "objects-keyed.json",
            "objects.json",
            "primitives.json",
            "whitespace.json"
        ]
        .map(str::to_owned)
        .into_iter()
        .collect()
    );
}
