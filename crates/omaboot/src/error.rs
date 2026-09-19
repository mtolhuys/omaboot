//! Error type.
//!
//! Every variant states what went wrong and what to do about it, because the
//! TUI will show one sentence and the log will carry the chain.

use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{what}. Suggested next step: {suggestion}")]
    Environment { what: String, suggestion: String },

    #[error(
        "refusing to write {path}: {owned} is owned by Omarchy and omaboot never writes inside it. \
         Suggested next step: report this as a bug, no user action can make this correct"
    )]
    OmarchyOwned { path: PathBuf, owned: String },

    #[error(
        "theme {name} was not found in {dir}. \
         Suggested next step: run `omaboot list` to see the themes you have, or `omaboot new {name}`"
    )]
    ThemeNotFound { name: String, dir: PathBuf },

    #[error(
        "{path} could not be read: {source}. \
         Suggested next step: check the file exists and that you can read it"
    )]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "{path} could not be written: {source}. \
         Suggested next step: check the directory exists and that you can write to it"
    )]
    WriteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "{path} is not valid TOML: {source}. \
         Suggested next step: fix the line the parser names, then run `omaboot validate` again"
    )]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error(
        "{path} is not a valid theme: {what}. \
         Suggested next step: {suggestion}"
    )]
    InvalidTheme {
        path: PathBuf,
        what: String,
        suggestion: String,
    },

    #[error(
        "asset reference {reference} is not usable: {why}. \
         Suggested next step: use a plain file name relative to the theme directory, such as logo.png"
    )]
    AssetReference { reference: String, why: String },

    #[error(
        "the template for {file} still contains the placeholder {{{{{placeholder}}}}}. \
         Suggested next step: report this as a bug, the generator is missing a value"
    )]
    TemplatePlaceholder { file: String, placeholder: String },

    #[error(
        "{value} cannot be written into {target} because it contains {what}. \
         Suggested next step: remove the character and try again"
    )]
    Unrepresentable {
        value: String,
        target: String,
        what: String,
    },

    #[error(
        "{tool} was not found on this system. \
         Suggested next step: {suggestion}"
    )]
    ToolMissing { tool: String, suggestion: String },

    #[error(
        "step {step} failed: {what}. \
         Suggested next step: {suggestion}"
    )]
    Step {
        step: String,
        what: String,
        suggestion: String,
    },

    #[error(
        "{command} exited with {code}: {stderr}. \
         Suggested next step: {suggestion}"
    )]
    Command {
        command: String,
        code: String,
        stderr: String,
        suggestion: String,
    },

    #[error(
        "there is no recorded rollback point in {path}. \
         Suggested next step: run `omaboot reset` to return to stock Omarchy instead"
    )]
    NoRollbackPoint { path: PathBuf },

    #[error(
        "the recorded state in {path} does not match the system: {what}. \
         Suggested next step: run `omaboot doctor` for the details, or `omaboot reset` to start clean"
    )]
    Drift { path: PathBuf, what: String },
}

impl Error {
    pub fn read(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::ReadFile {
            path: path.into(),
            source,
        }
    }

    pub fn write(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::WriteFile {
            path: path.into(),
            source,
        }
    }

    pub fn step(
        step: impl Into<String>,
        what: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self::Step {
            step: step.into(),
            what: what.into(),
            suggestion: suggestion.into(),
        }
    }
}
