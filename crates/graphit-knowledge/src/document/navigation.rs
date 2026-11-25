//! Navigation is authored metadata in the existing source, never a second store.
use super::*;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuthoredNavigation {
    pub previous: Option<String>,
    pub next: Option<String>,
}

// These are structural bounds derived from the already supplied source bytes,
// not product defaults. Each YAML event consumes syntax or one of the fixed
// stream/document delimiters. Text is bounded by source text, key ownership and
// the two selected scalar copies. Markdown is not parsed on this metadata path.
fn metadata<'a>(
    context: &SnapshotContext,
    source: &'a str,
) -> Result<(DocumentEnvelope, Metadata<'a>)> {
    context.check()?;
    let bytes = source.len().max(1);
    let cap =
        NonZeroUsize::new(bytes).ok_or(DocumentParseError::LimitExceeded("navigation bytes"))?;
    let events = bytes
        .checked_mul(2)
        .and_then(|value| value.checked_add(8))
        .and_then(NonZeroUsize::new)
        .ok_or(DocumentParseError::LimitExceeded("navigation events"))?;
    let text = bytes
        .checked_mul(4)
        .and_then(NonZeroUsize::new)
        .ok_or(DocumentParseError::LimitExceeded("navigation text"))?;
    let envelope = envelope(context, source, cap)?;
    let limits = DocumentParseLimits {
        max_document_bytes: cap,
        max_input_bytes: cap,
        max_frontmatter_bytes: cap,
        max_yaml_events: events,
        max_markdown_events: events,
        max_yaml_depth: cap,
        max_scalar_bytes: cap,
        max_links: events,
        max_projection_bytes: text,
        max_summary_bytes: cap,
    };
    let mut budget = TextBudget {
        context,
        limit: text,
        used: 0,
    };
    let parsed = match &envelope.yaml {
        Some(range) => parse_yaml(
            context,
            source,
            range.clone(),
            limits,
            &mut budget,
            &mut ParseStats::default(),
        )?,
        None => Metadata::default(),
    };
    Ok((envelope, parsed))
}

/// Parse just the existing YAML arena, retaining decoded authored destinations.
/// Absence, null and whitespace-only scalars mean no guide continuation.
pub fn document_navigation(context: &SnapshotContext, source: &str) -> Result<AuthoredNavigation> {
    let (_, metadata) = metadata(context, source)?;
    let copy = |name| -> Result<Option<String>> {
        editorial_field(context, &metadata, name)?
            .map(|value| {
                let mut output = String::new();
                output
                    .try_reserve_exact(value.len())
                    .map_err(|_| DocumentParseError::Request(io::ErrorKind::OutOfMemory.into()))?;
                output.push_str(value);
                Ok(output)
            })
            .transpose()
    };
    let result = AuthoredNavigation {
        previous: copy("previous")?,
        next: copy("next")?,
    };
    context.check()?;
    Ok(result)
}

/// Preserve every source byte except the two navigation entries. Documents
/// without those entries are returned borrowed and byte-identical.
pub fn without_document_navigation<'a>(
    context: &SnapshotContext,
    source: &'a str,
) -> Result<Cow<'a, str>> {
    let (envelope, metadata) = metadata(context, source)?;
    let Some(root) = metadata.root else {
        return Ok(Cow::Borrowed(source));
    };
    let MetadataKind::Mapping { entries } = &metadata.nodes[root.0].kind else {
        return Err(DocumentParseError::InvalidFieldType("frontmatter"));
    };
    let selected = entries.iter().map(|(key,_)| matches!(
        &metadata.nodes[key.0].kind, MetadataKind::Scalar {value,..} if value=="next" || value=="previous"
    )).collect::<Vec<_>>();
    if !selected.iter().any(|selected| *selected) {
        return Ok(Cow::Borrowed(source));
    }
    // Validate field types with precisely the same interpretation as publication.
    editorial_field(context, &metadata, "previous")?;
    editorial_field(context, &metadata, "next")?;
    let flow = source[metadata.nodes[root.0].span.clone()]
        .trim_start()
        .starts_with('{');
    let mut omitted = Vec::<Range<usize>>::new();
    for (index, (key, value)) in entries.iter().enumerate() {
        context.check()?;
        if !selected[index] {
            continue;
        }
        if flow && index > 0 && selected[index - 1] {
            continue;
        }
        let key = &metadata.nodes[key.0];
        let value = &metadata.nodes[value.0];
        let span = key.span.start..key.span.end.max(value.span.end);
        let range = if flow {
            let mut end = index + 1;
            while end < selected.len() && selected[end] {
                end += 1;
            }
            let (_, last) = entries[end - 1];
            let last_end = metadata.nodes[last.0].span.end.max(span.end);
            if let Some((next, _)) = entries.get(end) {
                span.start..metadata.nodes[next.0].span.start
            } else if index != 0 {
                let (_, previous) = entries[index - 1];
                metadata.nodes[previous.0].span.end..last_end
            } else {
                span.start..last_end
            }
        } else {
            let start = source[..span.start]
                .rfind('\n')
                .map_or(0, |index| index + 1);
            let end = if span.end > start && source.as_bytes().get(span.end - 1) == Some(&b'\n') {
                span.end
            } else {
                source[span.end..]
                    .find('\n')
                    .map_or(source.len(), |index| span.end + index + 1)
            };
            start..end
        };
        omitted.push(range);
    }
    omitted.sort_by_key(|range| range.start);
    let mut merged = Vec::<Range<usize>>::new();
    for range in omitted {
        if let Some(last) = merged.last_mut().filter(|last| range.start <= last.end) {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    // An envelope introduced solely for navigation has no embedding text.
    // Preserve independent comments instead of inferring their previous state.
    if selected.iter().all(|selected| *selected) {
        let yaml = envelope
            .yaml
            .as_ref()
            .ok_or(DocumentParseError::InvalidSpan)?;
        let mut remaining = String::new();
        let mut cursor = yaml.start;
        for range in &merged {
            let start = range.start.max(yaml.start);
            let end = range.end.min(yaml.end);
            if start > cursor {
                remaining.push_str(&source[cursor..start]);
            }
            cursor = cursor.max(end);
        }
        remaining.push_str(&source[cursor..yaml.end]);
        if remaining
            .chars()
            .all(|value| value.is_whitespace() || (flow && matches!(value, '{' | '}' | ',')))
        {
            merged = vec![
                envelope
                    .frontmatter
                    .ok_or(DocumentParseError::InvalidSpan)?,
            ];
        }
    }
    let mut output = String::new();
    output
        .try_reserve_exact(source.len())
        .map_err(|_| DocumentParseError::Request(io::ErrorKind::OutOfMemory.into()))?;
    let mut cursor = 0;
    for range in merged {
        context.check()?;
        output.push_str(&source[cursor..range.start]);
        cursor = range.end;
    }
    output.push_str(&source[cursor..]);
    context.check()?;
    Ok(Cow::Owned(output))
}
