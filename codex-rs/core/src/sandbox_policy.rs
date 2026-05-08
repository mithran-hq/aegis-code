use std::path::Path;

use codex_config::ConfigLayerStack;
use codex_config::RequirementSource;
use codex_config::SandboxModeRequirement;
use codex_config::sandbox_mode_requirement_for_permission_profile;
use codex_protocol::method_state::MethodSandboxPolicyStatus;
use codex_protocol::method_state::MethodSandboxPolicySummary;
use codex_protocol::method_state::MethodSandboxPosture;
use codex_protocol::models::PermissionProfile;
use codex_protocol::models::SandboxEnforcement;
use codex_protocol::permissions::FileSystemPath;

#[derive(Debug, Clone, PartialEq)]
pub struct SandboxPolicyContext {
    pub active_mode: Option<SandboxModeRequirement>,
    pub allowed_modes: Vec<SandboxModeRequirement>,
    pub source: Option<RequirementSource>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SandboxPolicyViolation {
    pub(crate) active_mode: Option<SandboxModeRequirement>,
    pub(crate) requested_mode: Option<SandboxModeRequirement>,
    pub(crate) allowed_modes: Vec<SandboxModeRequirement>,
    pub(crate) source: Option<RequirementSource>,
    pub(crate) reason: String,
}

impl SandboxPolicyContext {
    pub(crate) fn unrestricted(active_mode: SandboxModeRequirement) -> Self {
        Self {
            active_mode: Some(active_mode),
            allowed_modes: Vec::new(),
            source: None,
        }
    }

    pub(crate) fn is_configured(&self) -> bool {
        !self.allowed_modes.is_empty()
    }
}

pub(crate) fn sandbox_policy_context(
    permission_profile: &PermissionProfile,
    config_layer_stack: &ConfigLayerStack,
) -> SandboxPolicyContext {
    let active_mode = sandbox_mode_requirement_for_permission_profile(permission_profile);
    let requirements = config_layer_stack.requirements_toml();
    let source = config_layer_stack
        .requirements()
        .permission_profile
        .source
        .clone();
    match requirements.allowed_sandbox_modes.clone() {
        Some(allowed_modes) => SandboxPolicyContext {
            active_mode: Some(active_mode),
            allowed_modes,
            source,
        },
        None => SandboxPolicyContext::unrestricted(active_mode),
    }
}

pub(crate) fn evaluate_sandbox_policy(
    context: &SandboxPolicyContext,
    sandbox_bypass_requested: bool,
) -> Option<SandboxPolicyViolation> {
    if !context.is_configured() {
        return None;
    }

    let Some(active_mode) = context.active_mode else {
        return Some(SandboxPolicyViolation {
            active_mode: None,
            requested_mode: None,
            allowed_modes: context.allowed_modes.clone(),
            source: context.source.clone(),
            reason: "Aegis sandbox policy blocked protected workflow: sandbox posture is missing."
                .to_string(),
        });
    };

    if !context.allowed_modes.contains(&active_mode) {
        return Some(SandboxPolicyViolation {
            active_mode: Some(active_mode),
            requested_mode: None,
            allowed_modes: context.allowed_modes.clone(),
            source: context.source.clone(),
            reason: format!(
                "Aegis sandbox policy blocked protected workflow: active sandbox mode `{}` is not allowed by policy {}.",
                sandbox_mode_label(active_mode),
                allowed_modes_label(&context.allowed_modes)
            ),
        });
    }

    if sandbox_bypass_requested
        && !context
            .allowed_modes
            .contains(&SandboxModeRequirement::DangerFullAccess)
    {
        return Some(SandboxPolicyViolation {
            active_mode: Some(active_mode),
            requested_mode: Some(SandboxModeRequirement::DangerFullAccess),
            allowed_modes: context.allowed_modes.clone(),
            source: context.source.clone(),
            reason: format!(
                "Aegis sandbox policy blocked protected workflow: sandbox override requested `{}` but policy allows only {}.",
                sandbox_mode_label(SandboxModeRequirement::DangerFullAccess),
                allowed_modes_label(&context.allowed_modes)
            ),
        });
    }

    None
}

pub(crate) fn sandbox_posture_for_permission_profile(
    permission_profile: &PermissionProfile,
    cwd: &Path,
    config_layer_stack: &ConfigLayerStack,
) -> MethodSandboxPosture {
    let context = sandbox_policy_context(permission_profile, config_layer_stack);
    sandbox_posture_from_context(permission_profile, cwd, &context)
}

pub(crate) fn sandbox_posture_from_context(
    permission_profile: &PermissionProfile,
    cwd: &Path,
    context: &SandboxPolicyContext,
) -> MethodSandboxPosture {
    let mode = sandbox_mode_requirement_for_permission_profile(permission_profile);
    let policy = sandbox_policy_summary(context);
    MethodSandboxPosture {
        mode: sandbox_mode_label(mode).to_string(),
        permission_profile: permission_profile_summary(permission_profile, cwd),
        enforcement: enforcement_label(permission_profile.enforcement()).to_string(),
        network: if permission_profile.network_sandbox_policy().is_enabled() {
            "enabled".to_string()
        } else {
            "restricted".to_string()
        },
        policy: Some(policy),
    }
}

pub(crate) fn sandbox_policy_summary(context: &SandboxPolicyContext) -> MethodSandboxPolicySummary {
    if !context.is_configured() {
        return MethodSandboxPolicySummary {
            status: MethodSandboxPolicyStatus::Unrestricted,
            allowed_modes: Vec::new(),
            source: None,
            diagnostic: None,
        };
    }

    let status = match context.active_mode {
        Some(mode) if context.allowed_modes.contains(&mode) => MethodSandboxPolicyStatus::Allowed,
        Some(_) => MethodSandboxPolicyStatus::Blocked,
        None => MethodSandboxPolicyStatus::Missing,
    };
    let diagnostic = match (status, context.active_mode) {
        (MethodSandboxPolicyStatus::Allowed, Some(mode)) => Some(format!(
            "active sandbox mode `{}` is allowed by policy",
            sandbox_mode_label(mode)
        )),
        (MethodSandboxPolicyStatus::Blocked, Some(mode)) => Some(format!(
            "active sandbox mode `{}` is not allowed by policy {}",
            sandbox_mode_label(mode),
            allowed_modes_label(&context.allowed_modes)
        )),
        (MethodSandboxPolicyStatus::Missing, None) => {
            Some("sandbox posture is missing while sandbox policy is configured".to_string())
        }
        _ => None,
    };

    MethodSandboxPolicySummary {
        status,
        allowed_modes: context
            .allowed_modes
            .iter()
            .copied()
            .map(sandbox_mode_label)
            .map(str::to_string)
            .collect(),
        source: context.source.as_ref().map(ToString::to_string),
        diagnostic,
    }
}

pub(crate) fn sandbox_mode_label(mode: SandboxModeRequirement) -> &'static str {
    match mode {
        SandboxModeRequirement::ReadOnly => "read-only",
        SandboxModeRequirement::WorkspaceWrite => "workspace-write",
        SandboxModeRequirement::DangerFullAccess => "danger-full-access",
        SandboxModeRequirement::ExternalSandbox => "external-sandbox",
    }
}

pub(crate) fn allowed_modes_label(modes: &[SandboxModeRequirement]) -> String {
    let labels = modes
        .iter()
        .copied()
        .map(sandbox_mode_label)
        .collect::<Vec<_>>();
    format!("[{}]", labels.join(", "))
}

fn enforcement_label(enforcement: SandboxEnforcement) -> &'static str {
    match enforcement {
        SandboxEnforcement::Managed => "managed",
        SandboxEnforcement::Disabled => "disabled",
        SandboxEnforcement::External => "external",
    }
}

fn permission_profile_summary(permission_profile: &PermissionProfile, cwd: &Path) -> String {
    match permission_profile {
        PermissionProfile::Disabled => "danger-full-access".to_string(),
        PermissionProfile::External { .. } => {
            if permission_profile.network_sandbox_policy().is_enabled() {
                "external-sandbox (network access enabled)".to_string()
            } else {
                "external-sandbox".to_string()
            }
        }
        PermissionProfile::Managed { .. } => {
            let mode = sandbox_mode_requirement_for_permission_profile(permission_profile);
            let mut summary = sandbox_mode_label(mode).to_string();
            if mode == SandboxModeRequirement::WorkspaceWrite {
                summary.push_str(" [workdir");
                let writable_roots = permission_profile
                    .file_system_sandbox_policy()
                    .entries
                    .into_iter()
                    .filter(|entry| entry.access.can_write())
                    .filter_map(|entry| match entry.path {
                        FileSystemPath::Path { path } => Some(path.display().to_string()),
                        FileSystemPath::GlobPattern { .. } | FileSystemPath::Special { .. } => None,
                    })
                    .filter(|path| path != &cwd.display().to_string())
                    .collect::<Vec<_>>();
                for root in writable_roots {
                    summary.push_str(", ");
                    summary.push_str(&root);
                }
                summary.push(']');
            }
            if permission_profile.network_sandbox_policy().is_enabled() {
                summary.push_str(" (network access enabled)");
            }
            summary
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_config::ConfigLayerStack;
    use codex_config::ConfigRequirements;
    use codex_config::ConfigRequirementsToml;
    use codex_config::ConfigRequirementsWithSources;
    use codex_config::RequirementSource;
    use codex_config::Sourced;
    use codex_utils_absolute_path::AbsolutePathBuf;

    fn stack_with_allowed_modes(modes: Vec<SandboxModeRequirement>) -> ConfigLayerStack {
        let mut with_sources = ConfigRequirementsWithSources::default();
        with_sources.allowed_sandbox_modes = Some(Sourced::new(
            modes.clone(),
            RequirementSource::SystemRequirementsToml {
                file: AbsolutePathBuf::try_from("/tmp/requirements.toml").expect("absolute path"),
            },
        ));
        let requirements = ConfigRequirements::try_from(with_sources).expect("requirements");
        ConfigLayerStack::new(
            Vec::new(),
            requirements,
            ConfigRequirementsToml {
                allowed_sandbox_modes: Some(modes),
                ..ConfigRequirementsToml::default()
            },
        )
        .expect("stack")
    }

    #[test]
    fn allowed_workspace_write_posture_reports_policy_allowed() {
        let stack = stack_with_allowed_modes(vec![
            SandboxModeRequirement::ReadOnly,
            SandboxModeRequirement::WorkspaceWrite,
        ]);
        let posture = sandbox_posture_for_permission_profile(
            &PermissionProfile::workspace_write(),
            Path::new("/repo"),
            &stack,
        );

        assert_eq!(posture.mode, "workspace-write");
        assert_eq!(
            posture.policy.as_ref().map(|policy| policy.status),
            Some(MethodSandboxPolicyStatus::Allowed)
        );
    }

    #[test]
    fn sandbox_override_is_blocked_when_danger_full_access_is_not_allowed() {
        let stack = stack_with_allowed_modes(vec![
            SandboxModeRequirement::ReadOnly,
            SandboxModeRequirement::WorkspaceWrite,
        ]);
        let context = sandbox_policy_context(&PermissionProfile::workspace_write(), &stack);
        let violation = evaluate_sandbox_policy(&context, true).expect("violation");

        assert_eq!(
            violation.requested_mode,
            Some(SandboxModeRequirement::DangerFullAccess)
        );
        assert!(violation.reason.contains("sandbox override"));
    }
}
