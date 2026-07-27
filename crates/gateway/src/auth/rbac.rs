use super::Caller;

/// Permission definitions
///
/// Admin: full permissions
/// Member: read-only access to their own targets
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Permission {
    ViewTargets,
    CreateTarget,
    UpdateTarget,
    DeleteTarget,
    ViewIncidents,
    ResolveIncident,
    ManageMaintenance,
    ViewAuditLog,
    ManageUsers,
}

/// Decide whether the user has the given permission
#[allow(dead_code)]
pub fn has_permission(caller: &Caller, perm: Permission) -> bool {
    match caller.role.as_str() {
        "admin" => true,
        "member" => matches!(perm, Permission::ViewTargets | Permission::ViewIncidents),
        _ => false,
    }
}

/// Check whether the caller owns the resource
#[allow(dead_code)]
pub fn is_owner(caller: &Caller, owner_id: &str) -> bool {
    caller.user_id == owner_id
}

/// Members can only view resources they own
pub fn can_view_target(caller: &Caller, target_owner_id: &str) -> bool {
    caller.is_admin() || caller.user_id == target_owner_id
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admin() -> Caller {
        Caller {
            user_id: "u-admin".to_string(),
            email: "admin@example.com".to_string(),
            name: "Admin".to_string(),
            role: "admin".to_string(),
        }
    }

    fn member(user_id: &str) -> Caller {
        Caller {
            user_id: user_id.to_string(),
            email: format!("{}@example.com", user_id),
            name: user_id.to_string(),
            role: "member".to_string(),
        }
    }

    fn unknown_role() -> Caller {
        Caller {
            user_id: "u1".to_string(),
            email: "u1@example.com".to_string(),
            name: "U1".to_string(),
            role: "guest".to_string(), // Should never appear, but defensive code path
        }
    }

    // ── is_admin ──

    #[test]
    fn is_admin_true_only_for_admin_role() {
        assert!(admin().is_admin());
        assert!(!member("u1").is_admin());
        assert!(!unknown_role().is_admin());
    }

    // ── is_owner ──

    #[test]
    fn is_owner_compares_user_id_exactly() {
        let c = member("u1");
        assert!(is_owner(&c, "u1"));
        assert!(!is_owner(&c, "u2"));
        assert!(!is_owner(&c, "U1")); // Case-sensitive
        assert!(!is_owner(&c, ""));
    }

    // ── can_view_target ──

    #[test]
    fn admin_can_view_any_target() {
        let c = admin();
        assert!(can_view_target(&c, "u-other"));
        assert!(can_view_target(&c, "u-admin"));
        assert!(can_view_target(&c, ""));
    }

    #[test]
    fn member_can_view_only_own_targets() {
        let c = member("u1");
        assert!(can_view_target(&c, "u1"));
        assert!(!can_view_target(&c, "u2"));
    }

    // ── has_permission ──

    #[test]
    fn admin_has_every_permission() {
        let c = admin();
        for perm in [
            Permission::ViewTargets,
            Permission::CreateTarget,
            Permission::UpdateTarget,
            Permission::DeleteTarget,
            Permission::ViewIncidents,
            Permission::ResolveIncident,
            Permission::ManageMaintenance,
            Permission::ViewAuditLog,
            Permission::ManageUsers,
        ] {
            assert!(has_permission(&c, perm), "admin should have {:?}", perm);
        }
    }

    #[test]
    fn member_has_only_view_permissions() {
        let c = member("u1");
        assert!(has_permission(&c, Permission::ViewTargets));
        assert!(has_permission(&c, Permission::ViewIncidents));

        // Mutating operations are denied
        assert!(!has_permission(&c, Permission::CreateTarget));
        assert!(!has_permission(&c, Permission::UpdateTarget));
        assert!(!has_permission(&c, Permission::DeleteTarget));
        assert!(!has_permission(&c, Permission::ResolveIncident));
        assert!(!has_permission(&c, Permission::ManageMaintenance));
        assert!(!has_permission(&c, Permission::ManageUsers));

        // Admin-only views are denied
        assert!(!has_permission(&c, Permission::ViewAuditLog));
    }

    #[test]
    fn unknown_role_has_no_permissions() {
        let c = unknown_role();
        for perm in [
            Permission::ViewTargets,
            Permission::CreateTarget,
            Permission::ViewAuditLog,
        ] {
            assert!(!has_permission(&c, perm), "unknown role must not have {:?}", perm);
        }
    }
}
