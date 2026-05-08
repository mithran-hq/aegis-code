use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

pub const PLAN_LABEL: &str = "aegis-code:plan";
pub const TASK_LABEL: &str = "aegis-code:task";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueState {
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueSnapshot {
    pub number: u64,
    pub title: String,
    pub state: IssueState,
    pub body: String,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueTrainSnapshot {
    pub parent: IssueSnapshot,
    pub children: Vec<IssueSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParentChildRef {
    pub issue_number: u64,
    pub checked: bool,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueTrainFinding {
    pub severity: FindingSeverity,
    pub code: String,
    pub issue_number: Option<u64>,
    pub message: String,
    pub remediation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueTrainReport {
    pub valid: bool,
    pub parent_issue: u64,
    pub child_count: usize,
    pub findings: Vec<IssueTrainFinding>,
}

pub fn validate_issue_train(snapshot: &IssueTrainSnapshot) -> IssueTrainReport {
    let refs = parse_parent_child_refs(&snapshot.parent.body);
    let child_by_number = snapshot
        .children
        .iter()
        .map(|issue| (issue.number, issue))
        .collect::<BTreeMap<_, _>>();
    let mut findings = Vec::new();

    validate_parent(&snapshot.parent, &refs, &child_by_number, &mut findings);
    for child in &snapshot.children {
        validate_child(child, &mut findings);
    }

    let valid = !findings
        .iter()
        .any(|finding| finding.severity == FindingSeverity::Error);

    IssueTrainReport {
        valid,
        parent_issue: snapshot.parent.number,
        child_count: snapshot.children.len(),
        findings,
    }
}

pub fn parse_parent_child_refs(body: &str) -> Vec<ParentChildRef> {
    let child_issues_section = section_content_raw(body, "Child Issues").unwrap_or_default();
    child_issues_section
        .lines()
        .filter_map(parse_child_ref_line)
        .collect()
}

fn validate_parent(
    parent: &IssueSnapshot,
    refs: &[ParentChildRef],
    child_by_number: &BTreeMap<u64, &IssueSnapshot>,
    findings: &mut Vec<IssueTrainFinding>,
) {
    if !has_label(parent, PLAN_LABEL) {
        push_error(
            findings,
            Some(parent.number),
            "parent_missing_plan_label",
            "Parent plan issue is missing the aegis-code:plan label.",
            "Add the aegis-code:plan label to the coordination issue.",
        );
    }
    if has_label(parent, TASK_LABEL) {
        push_error(
            findings,
            Some(parent.number),
            "parent_has_task_label",
            "Parent plan issue is also labeled as an implementation task.",
            "Remove the aegis-code:task label from the parent issue.",
        );
    }

    require_nonempty_section(
        findings,
        parent.number,
        &parent.body,
        "Objective",
        "parent_missing_objective",
    );
    require_nonempty_section(
        findings,
        parent.number,
        &parent.body,
        "Child Issues",
        "parent_missing_child_issues",
    );
    if !has_heading_containing(&parent.body, "closure")
        && !has_heading_containing(&parent.body, "evidence")
    {
        push_error(
            findings,
            Some(parent.number),
            "parent_missing_closure_guidance",
            "Parent plan issue is missing closure or evidence guidance.",
            "Add a closure or evidence section that explains how child task completion is reconciled.",
        );
    }

    if refs.is_empty() {
        push_error(
            findings,
            Some(parent.number),
            "parent_missing_child_refs",
            "Parent plan issue does not list any child task issue references.",
            "Add a Child Issues checklist with one item per implementation task.",
        );
    }

    let mut ref_counts = BTreeMap::<u64, usize>::new();
    for child_ref in refs {
        *ref_counts.entry(child_ref.issue_number).or_default() += 1;
    }
    for (issue_number, count) in ref_counts {
        if count > 1 {
            push_error(
                findings,
                Some(parent.number),
                "parent_duplicate_child_ref",
                &format!(
                    "Parent plan issue references child issue #{issue_number} more than once."
                ),
                "Keep exactly one checklist entry per child task issue.",
            );
        }
    }

    for child_ref in refs {
        let Some(child) = child_by_number.get(&child_ref.issue_number) else {
            push_error(
                findings,
                Some(parent.number),
                "parent_unresolvable_child_ref",
                &format!(
                    "Parent plan issue references child issue #{} but it could not be loaded.",
                    child_ref.issue_number
                ),
                "Confirm the referenced child issue exists and is accessible.",
            );
            continue;
        };

        if let Some(title) = child_ref.title.as_deref() {
            if !titles_match(title, &child.title) {
                push_warning(
                    findings,
                    Some(parent.number),
                    "parent_child_title_mismatch",
                    &format!(
                        "Parent checklist title for #{} does not match the child issue title.",
                        child.number
                    ),
                    "Update the checklist text or child issue title so they describe the same task.",
                );
            }
        }

        let child_closed = child.state == IssueState::Closed;
        if child_ref.checked != child_closed {
            push_warning(
                findings,
                Some(parent.number),
                "parent_child_checkbox_drift",
                &format!(
                    "Parent checklist state for #{} does not match the child issue state.",
                    child.number
                ),
                "Update the parent checklist after the child issue state changes.",
            );
        }
    }

    let referenced = refs
        .iter()
        .map(|child_ref| child_ref.issue_number)
        .collect::<BTreeSet<_>>();
    for child_number in child_by_number.keys() {
        if !referenced.contains(child_number) {
            push_error(
                findings,
                Some(parent.number),
                "child_missing_parent_ref",
                &format!("Child issue #{child_number} is not listed in the parent plan issue."),
                "Add this child issue to the parent plan checklist or remove it from the validation set.",
            );
        }
    }
}

fn validate_child(child: &IssueSnapshot, findings: &mut Vec<IssueTrainFinding>) {
    if !has_label(child, TASK_LABEL) {
        push_error(
            findings,
            Some(child.number),
            "child_missing_task_label",
            "Child issue is missing the aegis-code:task label.",
            "Add the aegis-code:task label to implementation task issues.",
        );
    }
    if has_label(child, PLAN_LABEL) {
        push_error(
            findings,
            Some(child.number),
            "child_has_plan_label",
            "Child issue is also labeled as a parent plan.",
            "Remove the aegis-code:plan label from implementation task issues.",
        );
    }
    if !child.title.trim_start().starts_with("Task:") {
        push_error(
            findings,
            Some(child.number),
            "child_title_missing_task_prefix",
            "Child issue title does not start with `Task:`.",
            "Rename the child issue so implementation units are clearly distinguishable from plan issues.",
        );
    }

    let required_sections = [
        ("Objective", "child_missing_objective"),
        ("Scope", "child_missing_scope"),
        ("Acceptance Criteria", "child_missing_acceptance_criteria"),
        ("Falsifiers", "child_missing_falsifiers"),
        ("Dependencies", "child_missing_dependencies"),
    ];
    for (section, code) in required_sections {
        require_nonempty_section(findings, child.number, &child.body, section, code);
    }

    let objective = section_content_raw(&child.body, "Objective").unwrap_or_default();
    if is_vague_text(&objective) || is_generic_objective(&objective) {
        push_error(
            findings,
            Some(child.number),
            "child_vague_objective",
            "Child issue objective is vague or placeholder-like.",
            "Replace the objective with a concrete outcome that can be falsified.",
        );
    }

    for section in [
        "Objective",
        "Scope",
        "Acceptance Criteria",
        "Falsifiers",
        "Dependencies",
    ] {
        if let Some(content) = section_content_raw(&child.body, section) {
            if is_vague_text(&content) {
                push_error(
                    findings,
                    Some(child.number),
                    "child_vague_section",
                    &format!(
                        "Child issue section `{section}` contains placeholder or vague language."
                    ),
                    "Replace placeholders such as TBD, etc., various, or as needed with explicit task scope.",
                );
            }
        }
    }

    if bullet_count(section_content_raw(&child.body, "Acceptance Criteria").as_deref()) == 0 {
        push_error(
            findings,
            Some(child.number),
            "child_acceptance_criteria_without_bullets",
            "Acceptance Criteria section has no bullets.",
            "Add at least one bullet that states verifiable completion criteria.",
        );
    }
    if bullet_count(section_content_raw(&child.body, "Falsifiers").as_deref()) == 0 {
        push_error(
            findings,
            Some(child.number),
            "child_falsifiers_without_bullets",
            "Falsifiers section has no bullets.",
            "Add at least one bullet that would prove the implementation is not ready.",
        );
    }

    if let Some(dependencies) = section_content_raw(&child.body, "Dependencies") {
        if is_vague_text(&dependencies) {
            push_error(
                findings,
                Some(child.number),
                "child_dependencies_placeholder",
                "Dependencies section is a placeholder.",
                "State concrete dependencies or explicitly write `None`.",
            );
        }
    }

    let acceptance_bullets =
        bullet_count(section_content_raw(&child.body, "Acceptance Criteria").as_deref());
    let scope = section_content_raw(&child.body, "Scope").unwrap_or_default();
    let body_lower = child.body.to_lowercase();
    if acceptance_bullets > 8
        || scope.chars().count() > 1200
        || has_explicit_phase_list(&child.body)
        || body_lower.contains("multiple independently shippable")
        || body_lower.contains("independently shippable pieces")
        || body_lower.contains("separate commits")
    {
        push_error(
            findings,
            Some(child.number),
            "child_epic_sized",
            "Child issue appears too large for a single implementation unit.",
            "Split the issue into independently shippable child task issues before implementation.",
        );
    }
}

fn require_nonempty_section(
    findings: &mut Vec<IssueTrainFinding>,
    issue_number: u64,
    body: &str,
    section: &str,
    code: &str,
) {
    match section_content_raw(body, section) {
        Some(content) if !content.trim().is_empty() => {}
        _ => push_error(
            findings,
            Some(issue_number),
            code,
            &format!("Issue is missing a nonempty `{section}` section."),
            &format!("Add a `## {section}` section with implementation-ready content."),
        ),
    }
}

fn section_content_raw(body: &str, heading: &str) -> Option<String> {
    let mut lines = Vec::new();
    let mut in_section = false;
    let mut start_level = 0;

    for line in body.lines() {
        if let Some((level, title)) = parse_heading(line) {
            if in_section && level <= start_level {
                break;
            }
            if heading_matches(title, heading) {
                in_section = true;
                start_level = level;
                continue;
            }
        }

        if in_section {
            lines.push(line);
        }
    }

    in_section.then(|| lines.join("\n").trim().to_string())
}

fn parse_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if level == 0 {
        return None;
    }
    let rest = trimmed.get(level..)?;
    if !rest.starts_with(' ') {
        return None;
    }
    let title = rest.trim().trim_matches('#').trim();
    Some((level, title))
}

fn heading_matches(actual: &str, expected: &str) -> bool {
    normalize_title(actual) == normalize_title(expected)
}

fn has_heading_containing(body: &str, needle: &str) -> bool {
    let needle = needle.to_lowercase();
    body.lines()
        .filter_map(parse_heading)
        .any(|(_, title)| title.to_lowercase().contains(&needle))
}

fn parse_child_ref_line(line: &str) -> Option<ParentChildRef> {
    let trimmed = line.trim_start();
    let (checked, rest) = if let Some(rest) = trimmed.strip_prefix("- [ ]") {
        (false, rest)
    } else if let Some(rest) = trimmed.strip_prefix("- [x]") {
        (true, rest)
    } else if let Some(rest) = trimmed.strip_prefix("- [X]") {
        (true, rest)
    } else {
        return None;
    };

    let hash_index = rest.find('#')?;
    let after_hash = &rest[hash_index + 1..];
    let digit_count = after_hash
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .count();
    if digit_count == 0 {
        return None;
    }
    let issue_number = after_hash[..digit_count].parse().ok()?;
    let title = after_hash[digit_count..]
        .trim()
        .trim_start_matches(|ch: char| ch == '-' || ch == ':' || ch.is_whitespace())
        .trim();

    Some(ParentChildRef {
        issue_number,
        checked,
        title: (!title.is_empty()).then(|| title.to_string()),
    })
}

fn has_label(issue: &IssueSnapshot, label: &str) -> bool {
    issue.labels.iter().any(|candidate| candidate == label)
}

fn titles_match(parent_text: &str, child_title: &str) -> bool {
    normalize_title(parent_text) == normalize_title(strip_task_prefix(child_title))
}

fn strip_task_prefix(title: &str) -> &str {
    title
        .trim_start()
        .strip_prefix("Task:")
        .unwrap_or(title)
        .trim()
}

fn normalize_title(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|ch: char| ch == '-' || ch == ':' || ch.is_whitespace())
        .to_lowercase()
}

fn bullet_count(section: Option<&str>) -> usize {
    section
        .into_iter()
        .flat_map(str::lines)
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("- ") || trimmed.starts_with("* ")
        })
        .count()
}

fn is_vague_text(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|token| matches!(token, "tbd" | "todo" | "etc"))
        || lower.contains("as needed")
        || lower.contains("various")
}

fn is_generic_objective(text: &str) -> bool {
    let normalized = normalize_title(text);
    normalized.is_empty()
        || normalized == "implement task"
        || normalized == "do the work"
        || normalized == "fix issue"
        || normalized == "make changes"
}

fn has_explicit_phase_list(body: &str) -> bool {
    body.lines().any(|line| {
        let trimmed = line.trim_start();
        let lower = trimmed.to_lowercase();
        (trimmed.starts_with("- ") || trimmed.starts_with("* ") || starts_numbered_list(trimmed))
            && (lower.contains("phase 1")
                || lower.contains("phase one")
                || lower.contains("phase:"))
    })
}

fn starts_numbered_list(line: &str) -> bool {
    let digit_count = line.chars().take_while(|ch| ch.is_ascii_digit()).count();
    digit_count > 0
        && line
            .get(digit_count..)
            .is_some_and(|rest| rest.starts_with('.'))
}

fn push_error(
    findings: &mut Vec<IssueTrainFinding>,
    issue_number: Option<u64>,
    code: &str,
    message: &str,
    remediation: &str,
) {
    findings.push(IssueTrainFinding {
        severity: FindingSeverity::Error,
        code: code.to_string(),
        issue_number,
        message: message.to_string(),
        remediation: remediation.to_string(),
    });
}

fn push_warning(
    findings: &mut Vec<IssueTrainFinding>,
    issue_number: Option<u64>,
    code: &str,
    message: &str,
    remediation: &str,
) {
    findings.push(IssueTrainFinding {
        severity: FindingSeverity::Warning,
        code: code.to_string(),
        issue_number,
        message: message.to_string(),
        remediation: remediation.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parent(body: &str) -> IssueSnapshot {
        IssueSnapshot {
            number: 1,
            title: "Plan: Method workflow".to_string(),
            state: IssueState::Open,
            body: body.to_string(),
            labels: vec![PLAN_LABEL.to_string()],
        }
    }

    fn child(number: u64, title: &str, body: &str) -> IssueSnapshot {
        IssueSnapshot {
            number,
            title: title.to_string(),
            state: IssueState::Open,
            body: body.to_string(),
            labels: vec![TASK_LABEL.to_string()],
        }
    }

    fn valid_parent_body(checked: bool) -> String {
        let marker = if checked { "x" } else { " " };
        format!(
            "## Objective\n\nCoordinate the work.\n\n## Child Issues\n\n- [{marker}] #2 Implement validator\n\n## Evidence Required For Closure\n\nClose children after landed evidence is reconciled.\n"
        )
    }

    fn valid_child_body() -> &'static str {
        "## Objective\n\nShip a validator for issue trains.\n\n## Scope\n\nValidate parent and child task readiness.\n\n## Acceptance Criteria\n\n- Ready trains pass.\n- Invalid trains return actionable findings.\n\n## Falsifiers\n\n- Vague task issues pass.\n\n## Dependencies\n\nNone\n"
    }

    #[test]
    fn valid_ready_train_passes() {
        let snapshot = IssueTrainSnapshot {
            parent: parent(&valid_parent_body(false)),
            children: vec![child(2, "Task: Implement validator", valid_child_body())],
        };

        let report = validate_issue_train(&snapshot);

        assert!(report.valid, "{report:#?}");
        assert!(report.findings.is_empty());
        assert_eq!(report.parent_issue, 1);
        assert_eq!(report.child_count, 1);
    }

    #[test]
    fn missing_sections_and_falsifier_bullets_fail() {
        let body = "## Objective\n\nShip validator.\n\n## Scope\n\nTBD\n\n## Acceptance Criteria\n\nDone.\n\n## Dependencies\n\nNone\n";
        let snapshot = IssueTrainSnapshot {
            parent: parent(&valid_parent_body(false)),
            children: vec![child(2, "Task: Implement validator", body)],
        };

        let report = validate_issue_train(&snapshot);

        assert!(!report.valid);
        assert_has_code(&report, "child_missing_falsifiers");
        assert_has_code(&report, "child_acceptance_criteria_without_bullets");
        assert_has_code(&report, "child_vague_section");
    }

    #[test]
    fn vague_and_epic_child_fails() {
        let acceptance = (1..=9)
            .map(|index| format!("- Criterion {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let body = format!(
            "## Objective\n\nImplement task\n\n## Scope\n\nDo various work as needed.\n\n## Acceptance Criteria\n\n{acceptance}\n\n## Falsifiers\n\n- TBD\n\n## Dependencies\n\nTBD\n"
        );
        let snapshot = IssueTrainSnapshot {
            parent: parent(&valid_parent_body(false)),
            children: vec![child(2, "Implement validator", &body)],
        };

        let report = validate_issue_train(&snapshot);

        assert!(!report.valid);
        assert_has_code(&report, "child_title_missing_task_prefix");
        assert_has_code(&report, "child_vague_objective");
        assert_has_code(&report, "child_dependencies_placeholder");
        assert_has_code(&report, "child_epic_sized");
    }

    #[test]
    fn stale_parent_checkbox_is_warning_only() {
        let mut closed_child = child(2, "Task: Implement validator", valid_child_body());
        closed_child.state = IssueState::Closed;
        let snapshot = IssueTrainSnapshot {
            parent: parent(&valid_parent_body(false)),
            children: vec![closed_child],
        };

        let report = validate_issue_train(&snapshot);

        assert!(report.valid, "{report:#?}");
        assert_has_warning_code(&report, "parent_child_checkbox_drift");
    }

    #[test]
    fn duplicate_and_unresolvable_child_refs_fail() {
        let body = "## Objective\n\nCoordinate the work.\n\n## Child Issues\n\n- [ ] #2 Implement validator\n- [ ] #2 Implement validator\n- [ ] #3 Missing task\n\n## Closure\n\nReconcile child issues.\n";
        let snapshot = IssueTrainSnapshot {
            parent: parent(body),
            children: vec![child(2, "Task: Implement validator", valid_child_body())],
        };

        let report = validate_issue_train(&snapshot);

        assert!(!report.valid);
        assert_has_code(&report, "parent_duplicate_child_ref");
        assert_has_code(&report, "parent_unresolvable_child_ref");
    }

    #[test]
    fn label_mapping_drives_plan_and_task_validation() {
        let mut bad_parent = parent(&valid_parent_body(false));
        bad_parent.labels = vec![TASK_LABEL.to_string()];
        let mut bad_child = child(2, "Task: Implement validator", valid_child_body());
        bad_child.labels = vec![PLAN_LABEL.to_string()];
        let snapshot = IssueTrainSnapshot {
            parent: bad_parent,
            children: vec![bad_child],
        };

        let report = validate_issue_train(&snapshot);

        assert!(!report.valid);
        assert_has_code(&report, "parent_missing_plan_label");
        assert_has_code(&report, "parent_has_task_label");
        assert_has_code(&report, "child_missing_task_label");
        assert_has_code(&report, "child_has_plan_label");
    }

    fn assert_has_code(report: &IssueTrainReport, code: &str) {
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.severity == FindingSeverity::Error && finding.code == code),
            "missing error code {code}: {report:#?}"
        );
    }

    fn assert_has_warning_code(report: &IssueTrainReport, code: &str) {
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.severity == FindingSeverity::Warning
                    && finding.code == code),
            "missing warning code {code}: {report:#?}"
        );
    }
}
