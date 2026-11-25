use super::*;
use graphit_core::file_lock::Cancellation;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::{
    borrow::Cow,
    cell::Cell,
    collections::BTreeSet,
    io::{self, Write},
    time::Instant,
};

struct State<'a> {
    limits: MetadataLimits,
    cancellation: &'a Cancellation,
    deadline: Option<Instant>,
    nodes: Cell<usize>,
    failure: Cell<Option<MetadataError>>,
}
impl<'a> State<'a> {
    fn new(limits: MetadataLimits, context: &'a RequestContext<'_>) -> Result<Self, MetadataError> {
        limits.validate()?;
        let state = Self {
            limits,
            cancellation: context.cancellation(),
            deadline: context.deadline(),
            nodes: Cell::new(0),
            failure: Cell::new(None),
        };
        state.check()?;
        Ok(state)
    }
    fn check(&self) -> Result<(), MetadataError> {
        if self.cancellation.is_cancelled() {
            Err(MetadataError::Cancelled)
        } else if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            Err(MetadataError::Deadline)
        } else {
            Ok(())
        }
    }
    fn node(&self) -> Result<(), MetadataError> {
        self.check()?;
        let count = self
            .nodes
            .get()
            .checked_add(1)
            .filter(|n| *n <= self.limits.nodes)
            .ok_or(MetadataError::Limit)?;
        self.nodes.set(count);
        Ok(())
    }
    fn string(&self, value: &str) -> Result<(), MetadataError> {
        self.check()?;
        if value.len() > self.limits.scalar_bytes {
            Err(MetadataError::Limit)
        } else {
            Ok(())
        }
    }
    fn collection(&self, entries: usize, cap: usize) -> Result<(), MetadataError> {
        self.check()?;
        if entries > cap.min(self.limits.collection_entries) {
            Err(MetadataError::Limit)
        } else {
            Ok(())
        }
    }
    fn error<E: de::Error>(&self, error: MetadataError) -> E {
        if self.failure.get().is_none() {
            self.failure.set(Some(error));
        }
        E::custom("remote metadata rejected")
    }
    fn result<T, E: de::Error>(&self, value: Result<T, MetadataError>) -> Result<T, E> {
        value.map_err(|error| self.error(error))
    }
    fn invalid<E: de::Error>(&self) -> E {
        self.error(MetadataError::InvalidDocument)
    }
}

struct Scalar<'s, 'a, T> {
    state: &'s State<'a>,
    convert: fn(&str, &State<'_>) -> Result<T, MetadataError>,
}
impl<T> Copy for Scalar<'_, '_, T> {}
impl<T> Clone for Scalar<'_, '_, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'de, T> DeserializeSeed<'de> for Scalar<'_, '_, T> {
    type Value = T;
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<T, D::Error> {
        self.state.result(self.state.node())?;
        decoder.deserialize_str(self)
    }
}
impl<T> Visitor<'_> for Scalar<'_, '_, T> {
    type Value = T;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a bounded metadata string")
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<T, E> {
        self.state.result(self.state.string(value))?;
        self.state.result((self.convert)(value, self.state))
    }
}
fn text(value: &str, _: &State<'_>) -> Result<String, MetadataError> {
    Ok(value.into())
}
fn name(value: &str, _: &State<'_>) -> Result<String, MetadataError> {
    crate::projection::validate_name(value).map_err(|_| MetadataError::InvalidDocument)?;
    Ok(value.into())
}
fn username(value: &str, _: &State<'_>) -> Result<String, MetadataError> {
    if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
        return Err(MetadataError::InvalidDocument);
    }
    Ok(value.into())
}
fn identity(value: &str, state: &State<'_>) -> Result<ArtifactId, MetadataError> {
    ArtifactId::new(value, state.limits.projection).map_err(|_| MetadataError::InvalidDocument)
}
fn version(value: &str, state: &State<'_>) -> Result<ConcreteVersion, MetadataError> {
    validate_version(value, state)?;
    ConcreteVersion::new(value, state.limits.projection).map_err(|_| MetadataError::InvalidDocument)
}
fn validate_version(value: &str, state: &State<'_>) -> Result<(), MetadataError> {
    state
        .limits
        .projection
        .identity(value)
        .map_err(|_| MetadataError::InvalidDocument)?;
    if value.eq_ignore_ascii_case("latest") {
        return Err(MetadataError::InvalidDocument);
    }
    Ok(())
}
fn artifact_type(value: &str, _: &State<'_>) -> Result<ArtifactType, MetadataError> {
    ArtifactType::ALL
        .into_iter()
        .find(|kind| kind.as_str() == value)
        .ok_or(MetadataError::InvalidDocument)
}
fn producer(value: &str, _: &State<'_>) -> Result<ProjectId, MetadataError> {
    value.parse().map_err(|_| MetadataError::InvalidDocument)
}
fn hash(value: &str, _: &State<'_>) -> Result<[u8; 32], MetadataError> {
    if value.len() != 64 {
        return Err(MetadataError::InvalidDocument);
    }
    let mut output = [0; 32];
    fn digit(value: u8) -> Result<u8, MetadataError> {
        match value {
            b'0'..=b'9' => Ok(value - b'0'),
            b'a'..=b'f' => Ok(value - b'a' + 10),
            _ => Err(MetadataError::InvalidDocument),
        }
    }
    for (byte, pair) in output.iter_mut().zip(value.as_bytes().as_chunks::<2>().0) {
        *byte = digit(pair[0])? * 16 + digit(pair[1])?;
    }
    Ok(output)
}

struct Key<'s, 'a>(&'s State<'a>);
impl<'de> DeserializeSeed<'de> for Key<'_, '_> {
    type Value = Cow<'de, str>;
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<Self::Value, D::Error> {
        self.0.result(self.0.node())?;
        decoder.deserialize_str(self)
    }
}
impl<'de> Visitor<'de> for Key<'_, '_> {
    type Value = Cow<'de, str>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a metadata field")
    }
    fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<Self::Value, E> {
        self.0.result(self.0.string(value))?;
        Ok(Cow::Borrowed(value))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        self.0.result(self.0.string(value))?;
        Ok(Cow::Owned(value.into()))
    }
}
struct Refuse<'s, 'a>(&'s State<'a>);
impl<'de> DeserializeSeed<'de> for Refuse<'_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
        Err(self.0.error(MetadataError::Limit))
    }
}
fn next_key<'de, A: MapAccess<'de>>(
    map: &mut A,
    state: &State<'_>,
    fields: usize,
) -> Result<Option<Cow<'de, str>>, A::Error> {
    if fields == state.limits.collection_entries {
        map.next_key_seed(Refuse(state)).map(|_| None)
    } else {
        map.next_key_seed(Key(state))
    }
}
fn field<E: de::Error>(state: &State<'_>, seen: &mut u16, bit: u16) -> Result<(), E> {
    if *seen & bit != 0 {
        return Err(state.invalid());
    }
    *seen |= bit;
    Ok(())
}

#[derive(Clone, Copy)]
struct List<'s, 'a, S> {
    state: &'s State<'a>,
    item: S,
    cap: usize,
}
impl<'de, S: DeserializeSeed<'de> + Copy> DeserializeSeed<'de> for List<'_, '_, S> {
    type Value = Vec<S::Value>;
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<Self::Value, D::Error> {
        self.state.result(self.state.node())?;
        decoder.deserialize_seq(self)
    }
}
impl<'de, S: DeserializeSeed<'de> + Copy> Visitor<'de> for List<'_, '_, S> {
    type Value = Vec<S::Value>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a bounded metadata array")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let cap = self.cap.min(self.state.limits.collection_entries);
        let mut values = Vec::new();
        loop {
            self.state.result(self.state.check())?;
            if values.len() == cap {
                sequence.next_element_seed(Refuse(self.state))?;
                break;
            }
            let Some(value) = sequence.next_element_seed(self.item)? else {
                break;
            };
            // Do not let Vec's geometric capacity retain more slots than this array's cap.
            if values.len() == values.capacity() {
                let additional = values.capacity().max(1).min(cap - values.len());
                values
                    .try_reserve_exact(additional)
                    .map_err(|_| self.state.error(MetadataError::Limit))?;
            }
            values.push(value);
        }
        Ok(values)
    }
}

macro_rules! object_seed {
    ($name:ident, $output:ty) => {
        #[derive(Clone, Copy)]
        struct $name<'s, 'a>(&'s State<'a>);
        impl<'de> DeserializeSeed<'de> for $name<'_, '_> {
            type Value = $output;
            fn deserialize<D: de::Deserializer<'de>>(
                self,
                decoder: D,
            ) -> Result<Self::Value, D::Error> {
                self.0.result(self.0.node())?;
                decoder.deserialize_map(self)
            }
        }
    };
}
object_seed!(Author, String);
impl<'de> Visitor<'de> for Author<'_, '_> {
    type Value = String;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a metadata author")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut value = None;
        let mut fields = 0;
        while let Some(key) = next_key(&mut map, self.0, fields)? {
            fields += 1;
            if key != "username" || value.is_some() {
                return Err(self.0.invalid());
            }
            value = Some(map.next_value_seed(Scalar {
                state: self.0,
                convert: username,
            })?);
        }
        value.ok_or_else(|| self.0.invalid())
    }
}
object_seed!(Version, RemoteVersion);
impl<'de> Visitor<'de> for Version<'_, '_> {
    type Value = RemoteVersion;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a concrete version and digest")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let (mut version_value, mut hash_value) = (None, None);
        let (mut seen, mut fields) = (0, 0);
        while let Some(key) = next_key(&mut map, self.0, fields)? {
            fields += 1;
            match key.as_ref() {
                "version" => {
                    field(self.0, &mut seen, 1)?;
                    version_value = Some(map.next_value_seed(Scalar {
                        state: self.0,
                        convert: version,
                    })?);
                }
                "hash" => {
                    field(self.0, &mut seen, 2)?;
                    hash_value = Some(map.next_value_seed(Scalar {
                        state: self.0,
                        convert: hash,
                    })?);
                }
                _ => return Err(self.0.invalid()),
            }
        }
        Ok(RemoteVersion {
            version: version_value.ok_or_else(|| self.0.invalid())?,
            hash: hash_value.ok_or_else(|| self.0.invalid())?,
        })
    }
}
object_seed!(Dependency, RemoteDependency);
impl<'de> Visitor<'de> for Dependency<'_, '_> {
    type Value = RemoteDependency;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a metadata dependency")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let (mut kind, mut id, mut selector) = (None, None, None);
        let (mut seen, mut fields) = (0, 0);
        while let Some(key) = next_key(&mut map, self.0, fields)? {
            fields += 1;
            match key.as_ref() {
                "artifact_type" => {
                    field(self.0, &mut seen, 1)?;
                    kind = Some(map.next_value_seed(Scalar {
                        state: self.0,
                        convert: artifact_type,
                    })?);
                }
                "id" => {
                    field(self.0, &mut seen, 2)?;
                    id = Some(map.next_value_seed(Scalar {
                        state: self.0,
                        convert: identity,
                    })?);
                }
                "selector" => {
                    field(self.0, &mut seen, 4)?;
                    selector = Some(map.next_value_seed(Scalar {
                        state: self.0,
                        convert: text,
                    })?);
                }
                _ => return Err(self.0.invalid()),
            }
        }
        Ok(RemoteDependency {
            artifact_type: kind.ok_or_else(|| self.0.invalid())?,
            id: id.ok_or_else(|| self.0.invalid())?,
            selector: selector.ok_or_else(|| self.0.invalid())?,
        })
    }
}
object_seed!(Manifest, RemoteArtifactManifest);
impl<'de> Visitor<'de> for Manifest<'_, '_> {
    type Value = RemoteArtifactManifest;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a remote artifact manifest")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let (mut producer_value, mut kind, mut id, mut name_value) = (None, None, None, None);
        let (mut description, mut tags, mut author, mut latest) = (None, None, None, None);
        let (mut versions, mut dependencies) = (None, None);
        let (mut seen, mut fields) = (0, 0);
        while let Some(key) = next_key(&mut map, self.0, fields)? {
            fields += 1;
            match key.as_ref() {
                "v" => {
                    field(self.0, &mut seen, 1)?;
                    self.0.result(self.0.node())?;
                    let v: u64 = map.next_value()?;
                    if v != 1 {
                        return Err(self.0.invalid());
                    }
                }
                "producer" => {
                    field(self.0, &mut seen, 2)?;
                    producer_value = Some(map.next_value_seed(Scalar {
                        state: self.0,
                        convert: producer,
                    })?);
                }
                "artifact_type" => {
                    field(self.0, &mut seen, 4)?;
                    kind = Some(map.next_value_seed(Scalar {
                        state: self.0,
                        convert: artifact_type,
                    })?);
                }
                "id" => {
                    field(self.0, &mut seen, 8)?;
                    id = Some(map.next_value_seed(Scalar {
                        state: self.0,
                        convert: identity,
                    })?);
                }
                "name" => {
                    field(self.0, &mut seen, 16)?;
                    name_value = Some(map.next_value_seed(Scalar {
                        state: self.0,
                        convert: name,
                    })?);
                }
                "description" => {
                    field(self.0, &mut seen, 32)?;
                    description = Some(map.next_value_seed(Scalar {
                        state: self.0,
                        convert: text,
                    })?);
                }
                "tags" => {
                    field(self.0, &mut seen, 64)?;
                    tags = Some(map.next_value_seed(List {
                        state: self.0,
                        item: Scalar {
                            state: self.0,
                            convert: text,
                        },
                        cap: self.0.limits.collection_entries,
                    })?);
                }
                "author" => {
                    field(self.0, &mut seen, 128)?;
                    author = Some(map.next_value_seed(Author(self.0))?);
                }
                "latest" => {
                    field(self.0, &mut seen, 256)?;
                    latest = Some(map.next_value_seed(Scalar {
                        state: self.0,
                        convert: version,
                    })?);
                }
                "versions" => {
                    field(self.0, &mut seen, 512)?;
                    versions = Some(map.next_value_seed(List {
                        state: self.0,
                        item: Version(self.0),
                        cap: self.0.limits.versions,
                    })?);
                }
                "dependencies" => {
                    field(self.0, &mut seen, 1024)?;
                    dependencies = Some(map.next_value_seed(List {
                        state: self.0,
                        item: Dependency(self.0),
                        cap: self.0.limits.dependencies,
                    })?);
                }
                _ => return Err(self.0.invalid()),
            }
        }
        if seen & 1 == 0 {
            return Err(self.0.invalid());
        }
        let versions: Vec<RemoteVersion> = versions.ok_or_else(|| self.0.invalid())?;
        if versions.is_empty() {
            return Err(self.0.invalid());
        }
        let mut unique = BTreeSet::new();
        for entry in &versions {
            self.0.result(self.0.check())?;
            if !unique.insert(entry.version.as_str()) {
                return Err(self.0.invalid());
            }
        }
        if latest
            .as_ref()
            .is_some_and(|v: &ConcreteVersion| !unique.contains(v.as_str()))
        {
            return Err(self.0.invalid());
        }
        self.0.result(self.0.check())?;
        Ok(RemoteArtifactManifest {
            producer: producer_value.ok_or_else(|| self.0.invalid())?,
            artifact_type: kind.ok_or_else(|| self.0.invalid())?,
            id: id.ok_or_else(|| self.0.invalid())?,
            name: name_value.ok_or_else(|| self.0.invalid())?,
            description,
            tags: tags.unwrap_or_default(),
            author,
            latest,
            versions,
            dependencies: dependencies.ok_or_else(|| self.0.invalid())?,
        })
    }
}

pub(super) fn decode(
    bytes: &[u8],
    limits: MetadataLimits,
    context: &RequestContext<'_>,
) -> Result<RemoteArtifactManifest, MetadataError> {
    let state = State::new(limits, context)?;
    if bytes.len() > limits.bytes {
        return Err(MetadataError::Limit);
    }
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let output = Manifest(&state).deserialize(&mut decoder).map_err(|_| {
        state
            .failure
            .get()
            .unwrap_or(MetadataError::InvalidDocument)
    })?;
    decoder.end().map_err(|_| MetadataError::InvalidDocument)?;
    state.check()?;
    Ok(output)
}

#[derive(Serialize)]
struct Wire<'a, 'b> {
    v: u64,
    #[serde(flatten)]
    input: &'a ManifestInput<'b>,
}
struct CountWriter<'s, 'a> {
    state: &'s State<'a>,
    count: usize,
}
impl Write for CountWriter<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        // write_all retries Interrupted; permanent cancellation must end serialization.
        self.state.check().map_err(|error| {
            self.state.failure.set(Some(error));
            io::ErrorKind::Other
        })?;
        self.count = self
            .count
            .checked_add(bytes.len())
            .filter(|n| *n <= self.state.limits.bytes)
            .ok_or_else(|| {
                self.state.failure.set(Some(MetadataError::Limit));
                io::Error::from(io::ErrorKind::InvalidData)
            })?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct OutputWriter<'s, 'a> {
    state: &'s State<'a>,
    output: Vec<u8>,
    expected: usize,
}
impl Write for OutputWriter<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.state.check().map_err(|error| {
            self.state.failure.set(Some(error));
            io::ErrorKind::Other
        })?;
        if self
            .output
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.expected)
        {
            self.state.failure.set(Some(MetadataError::Limit));
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.output.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn scalar(state: &State<'_>, value: &str) -> Result<(), MetadataError> {
    state.node()?;
    state.string(value)
}
fn input_field(state: &State<'_>, key: &str) -> Result<(), MetadataError> {
    scalar(state, key)
}
fn input_string(state: &State<'_>, key: &str, value: &str) -> Result<(), MetadataError> {
    input_field(state, key)?;
    scalar(state, value)
}
fn preflight(input: &ManifestInput<'_>, state: &State<'_>) -> Result<(), MetadataError> {
    let fields = 7
        + usize::from(input.description.is_some())
        + usize::from(!input.tags.is_empty())
        + usize::from(input.author.is_some())
        + usize::from(input.latest.is_some());
    state.collection(fields, state.limits.collection_entries)?;
    state.node()?;
    input_field(state, "v")?;
    state.node()?;
    input_string(state, "producer", &input.producer.to_string())?;
    input_string(state, "artifact_type", input.artifact_type.as_str())?;
    input_string(state, "id", input.id)?;
    state
        .limits
        .projection
        .identity(input.id)
        .map_err(|_| MetadataError::InvalidDocument)?;
    for part in input.id.split('/') {
        crate::projection::validate_name(part).map_err(|_| MetadataError::InvalidDocument)?;
    }
    input_string(state, "name", input.name)?;
    crate::projection::validate_name(input.name).map_err(|_| MetadataError::InvalidDocument)?;
    if let Some(value) = input.description {
        input_string(state, "description", value)?;
    }
    if !input.tags.is_empty() {
        state.collection(input.tags.len(), state.limits.collection_entries)?;
        input_field(state, "tags")?;
        state.node()?;
        for value in input.tags {
            scalar(state, value)?;
        }
    }
    if let Some(value) = &input.author {
        state.collection(1, state.limits.collection_entries)?;
        input_field(state, "author")?;
        state.node()?;
        input_string(state, "username", value.username)?;
        if value.username.is_empty()
            || value.username.trim() != value.username
            || value.username.chars().any(char::is_control)
        {
            return Err(MetadataError::InvalidDocument);
        }
    }
    if let Some(value) = input.latest {
        input_string(state, "latest", value)?;
        validate_version(value, state)?;
    }
    state.collection(input.versions.len(), state.limits.versions)?;
    if input.versions.is_empty() {
        return Err(MetadataError::InvalidDocument);
    }
    input_field(state, "versions")?;
    state.node()?;
    state.collection(2, state.limits.collection_entries)?;
    let mut unique = BTreeSet::new();
    for entry in input.versions {
        state.node()?;
        input_string(state, "version", entry.version)?;
        validate_version(entry.version, state)?;
        input_string(state, "hash", entry.hash)?;
        hash(entry.hash, state)?;
        if !unique.insert(entry.version) {
            return Err(MetadataError::InvalidDocument);
        }
    }
    if input.latest.is_some_and(|v| !unique.contains(v)) {
        return Err(MetadataError::InvalidDocument);
    }
    state.collection(input.dependencies.len(), state.limits.dependencies)?;
    input_field(state, "dependencies")?;
    state.node()?;
    if !input.dependencies.is_empty() {
        state.collection(3, state.limits.collection_entries)?;
    }
    for entry in input.dependencies {
        state.node()?;
        input_string(state, "artifact_type", entry.artifact_type.as_str())?;
        input_string(state, "id", entry.id)?;
        state
            .limits
            .projection
            .identity(entry.id)
            .map_err(|_| MetadataError::InvalidDocument)?;
        for part in entry.id.split('/') {
            crate::projection::validate_name(part).map_err(|_| MetadataError::InvalidDocument)?;
        }
        input_string(state, "selector", entry.selector)?;
    }
    state.check()
}
pub(super) fn encode(
    input: &ManifestInput<'_>,
    limits: MetadataLimits,
    context: &RequestContext<'_>,
) -> Result<Vec<u8>, MetadataError> {
    let state = State::new(limits, context)?;
    preflight(input, &state)?;
    let wire = Wire { v: 1, input };
    let mut count = CountWriter {
        state: &state,
        count: 0,
    };
    serde_json::to_writer(&mut count, &wire).map_err(|_| {
        state
            .failure
            .get()
            .unwrap_or(MetadataError::InvalidDocument)
    })?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(count.count)
        .map_err(|_| MetadataError::Limit)?;
    let mut writer = OutputWriter {
        state: &state,
        output,
        expected: count.count,
    };
    serde_json::to_writer(&mut writer, &wire).map_err(|_| {
        state
            .failure
            .get()
            .unwrap_or(MetadataError::InvalidDocument)
    })?;
    if writer.output.len() != count.count {
        return Err(MetadataError::InvalidDocument);
    }
    state.check()?;
    Ok(writer.output)
}

#[cfg(test)]
mod writer_tests {
    use super::*;
    use crate::remote_metadata::tests::{context, input, limits};
    use std::{
        process::{Command, Stdio},
        thread,
        time::Duration,
    };

    const CASE: &str = "GRAPHIT_REMOTE_METADATA_WRITER_CASE";

    #[test]
    fn serialization_cancellation_and_deadline_stop_in_both_phases() {
        for case in [
            "count-cancel",
            "output-cancel",
            "count-deadline",
            "output-deadline",
        ] {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "remote_metadata::codec::writer_tests::serialization_stops_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env(CASE, case)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let bound = Instant::now() + Duration::from_secs(5);
            loop {
                if child.try_wait().unwrap().is_some() {
                    break;
                }
                if Instant::now() >= bound {
                    child.kill().unwrap();
                    let output = child.wait_with_output().unwrap();
                    panic!(
                        "writer fixture {case} exceeded its process bound: {} {}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
                thread::sleep(Duration::from_millis(5));
            }
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{case}: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            assert!(
                stdout.contains("1 passed"),
                "child selection did not execute the writer fixture: {stdout}"
            );
            assert!(
                stdout.contains(&format!("qualified {case}")),
                "child did not complete {case}: {stdout}"
            );
        }
    }

    #[test]
    #[ignore = "executed in four bounded subprocesses by the parent fixture"]
    fn serialization_stops_child() {
        let case = std::env::var(CASE).expect("bounded parent must choose a writer case");
        let cancellation = Cancellation::default();
        let deadline = Instant::now() + Duration::from_secs(1);
        let context = context(&cancellation, deadline);
        let state = State::new(limits(), &context).unwrap();
        let versions = [VersionInput {
            version: "1.2.3",
            hash: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        }];
        let input = input(ArtifactType::Skill, &versions, &[]);
        preflight(&input, &state).unwrap();
        let wire = Wire {
            v: 1,
            input: &input,
        };
        let mut count = CountWriter {
            state: &state,
            count: 0,
        };
        let output_phase = case.starts_with("output-");
        let mut output = if output_phase {
            serde_json::to_writer(&mut count, &wire).unwrap();
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(count.count).unwrap();
            Some(OutputWriter {
                state: &state,
                output: bytes,
                expected: count.count,
            })
        } else {
            None
        };
        let expected = match case.as_str() {
            "count-cancel" | "output-cancel" => {
                cancellation.cancel();
                MetadataError::Cancelled
            }
            "count-deadline" | "output-deadline" => {
                // Expire the original request after real preflight/count preparation.
                while Instant::now() < deadline {
                    thread::sleep(deadline.saturating_duration_since(Instant::now()));
                }
                MetadataError::Deadline
            }
            _ => panic!("unknown writer fixture case"),
        };
        let error = if let Some(writer) = &mut output {
            let error = serde_json::to_writer(&mut *writer, &wire).unwrap_err();
            assert!(writer.output.is_empty());
            error
        } else {
            let error = serde_json::to_writer(&mut count, &wire).unwrap_err();
            assert_eq!(count.count, 0);
            error
        };
        assert_eq!(error.io_error_kind(), Some(io::ErrorKind::Other));
        assert_eq!(state.failure.get(), Some(expected));
        assert_eq!(state.deadline, context.deadline());
        assert_eq!(context.deadline(), Some(deadline));
        println!("qualified {case}");
    }
}
