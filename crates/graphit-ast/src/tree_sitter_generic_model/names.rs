use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenericNameRepresentation {
    Identifier,
    Data { delimiter: Option<&'static str> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenericNameIssue {
    Empty,
    Rejected,
    NonExact,
    Recovery,
    BlankData,
    MultilineData,
    LongData,
}

#[derive(Clone, Copy)]
pub struct GenericName<'o> {
    raw: SourceRange<'o>,
    display: SourceRange<'o>,
    representation: GenericNameRepresentation,
}

impl<'o> GenericName<'o> {
    pub fn raw(&self) -> SourceRange<'o> {
        self.raw
    }

    pub fn display(&self) -> &'o str {
        self.display.text()
    }

    pub fn display_range(&self) -> SourceRange<'o> {
        self.display
    }

    pub fn representation(&self) -> GenericNameRepresentation {
        self.representation
    }
}

pub(super) fn name_view<'o>(
    source: &SourceText<'o>,
    raw: SourceRange<'o>,
    data: bool,
) -> Result<Result<GenericName<'o>, GenericNameIssue>, GenericModelError> {
    if raw.text().is_empty() {
        return Ok(Err(GenericNameIssue::Empty));
    }
    if !data {
        return Ok(Ok(GenericName {
            raw,
            display: raw,
            representation: GenericNameRepresentation::Identifier,
        }));
    }
    let delimiter = ["\"\"\"", "'''", "```", "\"", "'", "`"]
        .into_iter()
        .find(|delimiter| {
            raw.text().len() >= delimiter.len() * 2
                && raw.text().starts_with(*delimiter)
                && raw.text().ends_with(*delimiter)
        });
    let width = delimiter.map_or(0, str::len);
    let display = source.range(raw.start() + width, raw.end() - width)?;
    let issue = if display.text().trim().is_empty() {
        Some(GenericNameIssue::BlankData)
    } else if display.text().contains(['\r', '\n']) {
        Some(GenericNameIssue::MultilineData)
    } else if display.text().chars().count() > 256 {
        Some(GenericNameIssue::LongData)
    } else {
        None
    };
    Ok(match issue {
        Some(issue) => Err(issue),
        None => Ok(GenericName {
            raw,
            display,
            representation: GenericNameRepresentation::Data { delimiter },
        }),
    })
}
