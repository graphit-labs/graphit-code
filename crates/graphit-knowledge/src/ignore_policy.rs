//! Knowledge's protected ignore criteria, resolved without filesystem access.
//!
//! This projection is data, not authority. The caller must pair its configuration
//! with the physical request and let Core capture and revalidate controls.

use graphit_core::{config::ConfigSnapshot, file_io::SnapshotContext};
use std::{collections::TryReserveError, error::Error, fmt, io};

const BEFORE_BRAND: [&str; 70] = [
    "*.exe",
    "*.dll",
    "*.so",
    "*.dylib",
    "*.o",
    "*.a",
    "*.lib",
    "*.class",
    "*.jar",
    "*.war",
    "*.pyc",
    "*.pyo",
    "*.whl",
    "*.egg",
    "*.jpg",
    "*.jpeg",
    "*.png",
    "*.gif",
    "*.bmp",
    "*.ico",
    "*.svg",
    "*.webp",
    "*.mp3",
    "*.mp4",
    "*.avi",
    "*.mov",
    "*.mkv",
    "*.flv",
    "*.wav",
    "*.flac",
    "*.zip",
    "*.tar",
    "*.gz",
    "*.bz2",
    "*.7z",
    "*.rar",
    "*.xz",
    "*.pdf",
    "*.doc",
    "*.docx",
    "*.xls",
    "*.xlsx",
    "*.ppt",
    "*.pptx",
    "*.min.js",
    "*.min.css",
    "*.map",
    "*.lock",
    "package-lock.json",
    "yarn.lock",
    "Cargo.lock",
    "go.sum",
    "node_modules/",
    ".git/",
    "__pycache__/",
    ".venv/",
    "venv/",
    "vendor/",
    "dist/",
    "build/",
    ".cache/",
    "coverage/",
    ".idea/",
    ".vscode/",
    ".vs/",
    "*.swp",
    "*.swo",
    "*~",
    ".DS_Store",
    "Thumbs.db",
];
const AFTER_BRAND: [&str; 9] = [
    ".agents/",
    ".claude/",
    ".cursor/",
    ".kiro/",
    ".codex/",
    ".gemini/",
    ".opencode/",
    ".qwen/",
    ".kimi-code/",
];
const BRAND_INDEX: usize = BEFORE_BRAND.len();
const PATTERN_COUNT: usize = BEFORE_BRAND.len() + 1 + AFTER_BRAND.len();

#[derive(Debug)]
pub struct KnowledgeIgnorePolicy {
    patterns: Vec<String>,
}

#[derive(Debug)]
pub enum KnowledgeIgnorePolicyError {
    Context(io::Error),
    Allocation(TryReserveError),
}

impl KnowledgeIgnorePolicyError {
    pub fn kind(&self) -> io::ErrorKind {
        match self {
            Self::Context(error) => error.kind(),
            Self::Allocation(_) => io::ErrorKind::OutOfMemory,
        }
    }
}

impl fmt::Display for KnowledgeIgnorePolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(_) => f.write_str("Knowledge ignore policy request is not live"),
            Self::Allocation(_) => f.write_str("cannot allocate Knowledge ignore policy"),
        }
    }
}

impl Error for KnowledgeIgnorePolicyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Context(error) => Some(error),
            Self::Allocation(error) => Some(error),
        }
    }
}

type Result<T> = std::result::Result<T, KnowledgeIgnorePolicyError>;

impl KnowledgeIgnorePolicy {
    /// Resolve the complete protected defaults from an explicit configuration.
    ///
    /// No configuration key, environment, path or file is read here. Checks use
    /// the exact context supplied by the caller and do not reserve any I/O.
    pub fn resolve(context: &SnapshotContext, snapshot: &ConfigSnapshot) -> Result<Self> {
        check(context)?;
        let directory = snapshot.brand().directory();
        // Brand validates a 1–64 byte key upstream; dot + key + slash is at most 66.
        let brand_bytes = directory.len() + 1;

        check(context)?;
        let mut patterns = Vec::new();
        patterns
            .try_reserve_exact(PATTERN_COUNT)
            .map_err(KnowledgeIgnorePolicyError::Allocation)?;
        for pattern in BEFORE_BRAND {
            append(context, &mut patterns, pattern)?;
        }

        check(context)?;
        let mut brand_pattern = String::new();
        brand_pattern
            .try_reserve_exact(brand_bytes)
            .map_err(KnowledgeIgnorePolicyError::Allocation)?;
        check(context)?;
        brand_pattern.push_str(directory);
        brand_pattern.push('/');
        patterns.push(brand_pattern);

        for pattern in AFTER_BRAND {
            append(context, &mut patterns, pattern)?;
        }
        check(context)?;
        Ok(Self { patterns })
    }

    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }

    pub fn brand_directory(&self) -> &str {
        self.patterns[BRAND_INDEX]
            .strip_suffix('/')
            .expect("the resolved brand pattern has a trailing slash")
    }

    pub const fn custom_filename(&self) -> &'static str {
        ".wikiignore"
    }

    pub const fn git_filename(&self) -> &'static str {
        ".gitignore"
    }
}

fn check(context: &SnapshotContext) -> Result<()> {
    context.check().map_err(KnowledgeIgnorePolicyError::Context)
}

fn append(context: &SnapshotContext, patterns: &mut Vec<String>, pattern: &str) -> Result<()> {
    check(context)?;
    let mut owned = String::new();
    owned
        .try_reserve_exact(pattern.len())
        .map_err(KnowledgeIgnorePolicyError::Allocation)?;
    check(context)?;
    owned.push_str(pattern);
    patterns.push(owned);
    Ok(())
}
