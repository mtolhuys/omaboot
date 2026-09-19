//! What a run did, or would do.

use std::fmt::Write as _;

use super::{Operation, StepId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepReport {
    pub id: Option<StepId>,
    pub title: String,
    pub operations: Vec<Operation>,
    pub skipped: bool,
    pub problems: Vec<String>,
}

impl StepReport {
    pub fn new(id: StepId, operations: Vec<Operation>) -> Self {
        Self {
            id: Some(id),
            title: id.title().to_string(),
            operations,
            skipped: false,
            problems: Vec::new(),
        }
    }

    pub fn named(title: &str, operations: Vec<Operation>) -> Self {
        Self {
            id: None,
            title: title.to_string(),
            operations,
            skipped: false,
            problems: Vec::new(),
        }
    }

    pub fn with_problem(id: StepId, operations: Vec<Operation>, problem: &str) -> Self {
        let mut report = Self::new(id, operations);
        report.problems.push(problem.to_string());
        report
    }

    pub fn skipped(id: StepId) -> Self {
        Self {
            id: Some(id),
            title: id.title().to_string(),
            operations: Vec::new(),
            skipped: true,
            problems: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyReport {
    pub theme: String,
    pub theme_hash: String,
    pub dry_run: bool,
    pub reverted: bool,
    pub steps: Vec<StepReport>,
}

impl ApplyReport {
    pub fn new(theme: &str, dry_run: bool) -> Self {
        Self {
            theme: theme.to_string(),
            theme_hash: String::new(),
            dry_run,
            reverted: false,
            steps: Vec::new(),
        }
    }

    pub fn push(&mut self, step: StepReport) {
        self.steps.push(step);
    }

    pub fn operations(&self) -> impl Iterator<Item = &Operation> {
        self.steps.iter().flat_map(|step| step.operations.iter())
    }

    pub fn problems(&self) -> Vec<&str> {
        self.steps
            .iter()
            .flat_map(|step| step.problems.iter().map(String::as_str))
            .collect()
    }

    /// The exact list of operations, grouped by step.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let verb = if self.dry_run {
            "would perform"
        } else {
            "performed"
        };
        let _ = writeln!(out, "omaboot: theme {} ({})", self.theme, verb);
        if !self.theme_hash.is_empty() {
            let _ = writeln!(
                out,
                "         theme hash {}",
                self.theme_hash.chars().take(16).collect::<String>()
            );
        }
        let _ = writeln!(out);

        for step in &self.steps {
            if step.skipped {
                let _ = writeln!(out, "- {} (not selected)", step.title);
                continue;
            }
            let _ = writeln!(out, "- {}", step.title);
            for operation in &step.operations {
                let _ = writeln!(out, "    {operation}");
            }
            for problem in &step.problems {
                let _ = writeln!(out, "    ! {problem}");
            }
        }

        let problems = self.problems();
        if !problems.is_empty() {
            let _ = writeln!(out);
            let _ = writeln!(out, "{} problem(s) would stop a real run:", problems.len());
            for problem in problems {
                let _ = writeln!(out, "  - {problem}");
            }
        }
        if self.reverted {
            let _ = writeln!(out);
            let _ = writeln!(out, "the rollback point was restored");
        }
        out
    }
}
