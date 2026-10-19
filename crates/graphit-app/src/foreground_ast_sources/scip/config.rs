//! Extension routes to exact Docker image references, using the common config layers.
use graphit_core::config::{ConfigKey, ConfigSnapshot};
use std::{collections::BTreeMap, io, path::Path};

#[derive(Clone, Debug, Default)]
pub(crate) struct CustomParsers {
    images: BTreeMap<String, String>,
}

impl CustomParsers {
    pub(crate) fn resolve(config: &ConfigSnapshot) -> io::Result<Self> {
        let configured = config
            .get::<String>(&ConfigKey::new("ast.scip.parsers")?)?
            .unwrap_or_default();
        let mut images = BTreeMap::new();
        if configured.trim().is_empty() {
            return Ok(Self { images });
        }
        for item in configured.split(',') {
            let (extension, image) = item.trim().split_once('=').ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "ast.scip.parsers requires comma-separated extension=image items without empty segments",
                )
            })?;
            let extension = normalize_extension(extension.trim())?;
            let image = image.trim();
            // Docker is the image-reference authority. This is one argument, never a
            // command, URL or list of CLI options; tags and digests stay unchanged.
            if image.is_empty()
                || image.starts_with('-')
                || image.contains("://")
                || image.chars().any(|c| c.is_whitespace() || c.is_control())
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "ast.scip.parsers images must be single Docker references, without URL schemes or CLI options",
                ));
            }
            if images.insert(extension, image.to_owned()).is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "ast.scip.parsers contains colliding normalized extensions",
                ));
            }
        }
        Ok(Self { images })
    }

    pub(crate) fn get(&self, path: &Path) -> Option<&str> {
        let mut extension = format!(".{}", path.extension()?.to_str()?);
        extension.make_ascii_lowercase();
        self.images.get(&extension).map(String::as_str)
    }

    /// Conventional dotted, ASCII-case-normalized extensions.
    pub(crate) fn extensions(&self) -> impl Iterator<Item = &str> {
        self.images.keys().map(String::as_str)
    }
}

fn normalize_extension(extension: &str) -> io::Result<String> {
    let suffix = extension.strip_prefix('.').unwrap_or(extension);
    if suffix.is_empty()
        || suffix
            .chars()
            .any(|c| matches!(c, '.' | '/' | '\\' | ':') || c.is_whitespace() || c.is_control())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ast.scip.parsers keys must be single file extensions with an optional leading dot",
        ));
    }
    Ok(format!(".{}", suffix.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::{
        brand::Brand,
        config::{ConfigDocument, ConfigSource},
    };
    use serde_json::{Value, json};

    fn layer(source: ConfigSource, value: Value) -> ConfigDocument {
        ConfigDocument::new(
            &Brand::default(),
            source,
            [(ConfigKey::new("ast.scip.parsers").unwrap(), value)],
        )
        .unwrap()
    }

    fn resolve(value: Value) -> io::Result<CustomParsers> {
        CustomParsers::resolve(
            &ConfigSnapshot::new(&Brand::default(), [layer(ConfigSource::Project, value)]).unwrap(),
        )
    }

    #[test]
    fn absent_setting_is_empty_and_routes_preserve_exact_image_references() {
        let empty = ConfigSnapshot::new(&Brand::default(), []).unwrap();
        assert_eq!(
            CustomParsers::resolve(&empty).unwrap().extensions().count(),
            0
        );
        let parsers = resolve(json!(
            " ZIG = localhost:5000/team/scip-zig:dev , .ExM = registry.example/team/parser@sha256:0123456789abcdef "
        ))
        .unwrap();
        assert_eq!(parsers.extensions().collect::<Vec<_>>(), [".exm", ".zig"]);
        assert_eq!(
            parsers.get(Path::new("src/example.ZiG")),
            Some("localhost:5000/team/scip-zig:dev")
        );
        assert_eq!(
            parsers.get(Path::new("name with spaces/ação.exm")),
            Some("registry.example/team/parser@sha256:0123456789abcdef")
        );
        assert_eq!(parsers.get(Path::new("src/unknown.rs")), None);
        assert_eq!(parsers.get(Path::new("src/no-extension")), None);
    }

    #[test]
    fn invalid_types_extensions_collisions_and_image_arguments_fail() {
        for value in [json!(1), json!([]), json!({"zig": "image"})] {
            assert!(resolve(value).is_err());
        }
        for extension in [
            "", ".", "..zig", "tar.gz", "a/b", "a\\b", "a:b", "z ig", "a\nb",
        ] {
            assert!(resolve(json!(format!("{extension}=team/parser:1"))).is_err());
        }
        assert!(resolve(json!("zig=one,.ZiG=two")).is_err());
        for image in [
            "",
            "--privileged",
            "im\nage",
            "image\0",
            "image --flag",
            "https://registry/image",
            "docker://image",
        ] {
            assert!(resolve(json!(format!("zig={image}"))).is_err());
        }
        for value in [
            "image",
            "zig",
            "zig=image,",
            ",zig=image",
            "zig=image,,rs=other",
            ",",
        ] {
            assert!(resolve(json!(value)).is_err(), "{value}");
        }
    }

    #[test]
    fn winning_layer_replaces_the_whole_list_and_empty_string_clears_routes() {
        let brand = Brand::default();
        let sources = [
            ConfigSource::Command,
            ConfigSource::Environment,
            ConfigSource::Project,
            ConfigSource::Global,
            ConfigSource::Distribution,
        ];
        for highest in 0..sources.len() {
            let layers = sources[highest..]
                .iter()
                .enumerate()
                .map(|(index, source)| layer(*source, json!(format!("ext{index}=parser:{index}"))));
            let config = ConfigSnapshot::new(&brand, layers).unwrap();
            let parsers = CustomParsers::resolve(&config).unwrap();
            assert_eq!(parsers.extensions().collect::<Vec<_>>(), [".ext0"]);
            assert_eq!(parsers.get(Path::new("a.ext0")), Some("parser:0"));
        }
        let config = ConfigSnapshot::new(
            &brand,
            [
                layer(ConfigSource::Global, json!("zig=parser:1")),
                layer(ConfigSource::Project, json!("")),
            ],
        )
        .unwrap();
        assert_eq!(
            CustomParsers::resolve(&config)
                .unwrap()
                .extensions()
                .count(),
            0
        );
        let invalid = ConfigSnapshot::new(
            &brand,
            [
                layer(ConfigSource::Global, json!("zig=parser:1")),
                layer(ConfigSource::Project, json!("zig=")),
            ],
        )
        .unwrap();
        assert!(CustomParsers::resolve(&invalid).is_err());
    }

    #[test]
    fn canonical_environment_uses_the_existing_capture_and_string_setting() {
        let brand = Brand::default();
        let key = ConfigKey::new("ast.scip.parsers").unwrap();
        assert_eq!(
            key.environment_variable(&brand),
            "GRAPHIT_AST__SCIP__PARSERS"
        );
        let environment = ConfigSnapshot::capture_environment(
            &brand,
            &[key],
            [(
                "GRAPHIT_AST__SCIP__PARSERS".into(),
                ".zig=team/parser:1".into(),
            )],
        )
        .unwrap();
        let config = ConfigSnapshot::new(
            &brand,
            [
                environment,
                layer(ConfigSource::Project, json!("exm=other:2")),
            ],
        )
        .unwrap();
        let parsers = CustomParsers::resolve(&config).unwrap();
        assert_eq!(parsers.get(Path::new("file.zig")), Some("team/parser:1"));
        assert_eq!(parsers.get(Path::new("file.exm")), None);
    }
}
