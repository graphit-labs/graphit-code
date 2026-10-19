//! Task readiness and closure over immutable borrowed facts.
#![doc = include_str!("../../../docs/specs/rust_record_gates.md")]
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    InProgress,
    Completed,
    Cancelled,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckKind {
    Acceptance,
    Test,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckStatus {
    Pending,
    Passed,
    Failed,
    Superseded,
}
pub struct CheckFact<'a> {
    pub kind: CheckKind,
    pub status: CheckStatus,
    pub evidence: &'a str,
}
pub struct TaskFact<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub parent_id: Option<&'a str>,
    pub status: TaskStatus,
    pub flagged: bool,
    pub dependencies: &'a [&'a str],
    pub checks: &'a [CheckFact<'a>],
}
/// Errors contain no record bodies, identifiers or tokens.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateError {
    InvalidFacts,
    MissingTask,
}
impl fmt::Display for GateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidFacts => "invalid record gate facts or identifier",
            Self::MissingTask => "task absent from record gate facts",
        })
    }
}
impl std::error::Error for GateError {}

/// An immutable evaluation. This is not evidence of authority or complete capture.
///
/// The facts must outlive the evaluation:
/// ```compile_fail
/// use graphit_records::gates::{GateGraph, TaskFact, TaskStatus};
/// fn escape() -> GateGraph<'static> {
///     let facts = [TaskFact { id: "task", session_id: "session", parent_id: None,
///         status: TaskStatus::Open, flagged: false, dependencies: &[], checks: &[] }];
///     GateGraph::new(&facts).unwrap()
/// }
/// ```
pub struct GateGraph<'a> {
    facts: &'a [TaskFact<'a>],
    index: HashMap<&'a str, usize>,
    conditions: Vec<bool>,
    valid_complete: Vec<bool>,
}

#[derive(Clone, Copy)]
enum EdgeKind {
    Dependency,
    Child,
}

fn identifier(id: &str) -> Result<(), GateError> {
    if id.is_empty() || id.trim() != id {
        Err(GateError::InvalidFacts)
    } else {
        Ok(())
    }
}
fn checks_pass(fact: &TaskFact<'_>) -> bool {
    if fact.flagged {
        return false;
    }
    let (mut acceptance, mut test) = (false, false);
    for check in fact.checks {
        if check.status == CheckStatus::Superseded {
            continue;
        }
        match check.kind {
            CheckKind::Acceptance => acceptance = true,
            CheckKind::Test => test = true,
        }
        if check.status != CheckStatus::Passed || check.evidence.trim().is_empty() {
            return false;
        }
    }
    acceptance && test
}

impl<'a> GateGraph<'a> {
    /// Validates the combined graph and evaluates it without recursive traversal.
    pub fn new(facts: &'a [TaskFact<'a>]) -> Result<Self, GateError> {
        let mut index = HashMap::with_capacity(facts.len());
        for (ordinal, fact) in facts.iter().enumerate() {
            for id in [Some(fact.id), Some(fact.session_id), fact.parent_id]
                .into_iter()
                .flatten()
                .chain(fact.dependencies.iter().copied())
            {
                identifier(id)?;
            }
            if index.insert(fact.id, ordinal).is_some() {
                return Err(GateError::InvalidFacts);
            }
        }
        let mut conditions: Vec<bool> = facts.iter().map(checks_pass).collect();
        let mut dependents = vec![Vec::new(); facts.len()];
        let mut remaining = vec![0usize; facts.len()];
        let mut seen = vec![usize::MAX; facts.len()];
        for (ordinal, fact) in facts.iter().enumerate() {
            for dependency in fact.dependencies {
                let source = *index.get(dependency).ok_or(GateError::InvalidFacts)?;
                if source == ordinal || seen[source] == ordinal {
                    return Err(GateError::InvalidFacts);
                }
                seen[source] = ordinal;
                // Withdrawn work has no prerequisite ordering. Its descendants
                // still propagate through the independent hierarchy edges.
                if fact.status == TaskStatus::Cancelled {
                    continue;
                }
                // Cancellation can never satisfy a prerequisite. Its false result
                // is known without waiting for this source's hierarchy evaluation.
                if facts[source].status == TaskStatus::Cancelled {
                    conditions[ordinal] = false;
                    continue;
                }
                dependents[source].push((ordinal, EdgeKind::Dependency));
                remaining[ordinal] += 1;
            }
            if let Some(parent) = fact.parent_id {
                let target = *index.get(parent).ok_or(GateError::InvalidFacts)?;
                if target == ordinal || fact.session_id != facts[target].session_id {
                    return Err(GateError::InvalidFacts);
                }
                // A prerequisite with non-cancelled endpoints can also be a child edge.
                dependents[ordinal].push((target, EdgeKind::Child));
                remaining[target] += 1;
            }
        }
        let mut valid_complete = vec![false; facts.len()];
        let mut children_closed = vec![true; facts.len()];
        let mut queue: Vec<usize> = remaining
            .iter()
            .enumerate()
            .filter_map(|(ordinal, &count)| (count == 0).then_some(ordinal))
            .collect();
        let mut cursor = 0;
        while cursor < queue.len() {
            let ordinal = queue[cursor];
            cursor += 1;
            valid_complete[ordinal] =
                conditions[ordinal] && facts[ordinal].status == TaskStatus::Completed;
            for &(target, kind) in &dependents[ordinal] {
                let satisfied = match kind {
                    EdgeKind::Dependency => valid_complete[ordinal],
                    EdgeKind::Child => {
                        valid_complete[ordinal]
                            || facts[ordinal].status == TaskStatus::Cancelled
                                && children_closed[ordinal]
                    }
                };
                conditions[target] &= satisfied;
                if matches!(kind, EdgeKind::Child) {
                    children_closed[target] &= satisfied;
                }
                remaining[target] -= 1;
                if remaining[target] == 0 {
                    queue.push(target);
                }
            }
        }
        if cursor != facts.len() {
            return Err(GateError::InvalidFacts);
        }
        Ok(Self {
            facts,
            index,
            conditions,
            valid_complete,
        })
    }
    fn lookup(&self, id: &str) -> Result<usize, GateError> {
        identifier(id)?;
        self.index.get(id).copied().ok_or(GateError::MissingTask)
    }
    /// Open tasks require validly completed dependencies; own checks and children do not block readiness.
    pub fn ready(&self, id: &str) -> Result<bool, GateError> {
        let ordinal = self.lookup(id)?;
        let fact = &self.facts[ordinal];
        Ok(fact.status == TaskStatus::Open
            && fact
                .dependencies
                .iter()
                .all(|dependency| self.valid_complete[self.index[dependency]]))
    }
    /// Closure conditions only. The authoritative service still validates all mutation fences.
    pub fn completion(&self, id: &str) -> Result<bool, GateError> {
        let ordinal = self.lookup(id)?;
        Ok(self.facts[ordinal].status != TaskStatus::Cancelled && self.conditions[ordinal])
    }
    /// No matching facts yields conditional true; this does not establish session existence.
    pub fn session_closable(&self, session_id: &str) -> Result<bool, GateError> {
        identifier(session_id)?;
        Ok(self
            .facts
            .iter()
            .enumerate()
            .filter(|(_, fact)| fact.session_id == session_id)
            .all(|(ordinal, fact)| {
                fact.status == TaskStatus::Cancelled || self.valid_complete[ordinal]
            }))
    }
}
