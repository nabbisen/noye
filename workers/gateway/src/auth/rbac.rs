use super::Caller;

/// 権限定義
///
/// 管理者 (admin): 全操作可能
/// 会員 (member): 自身が所有する監視対象の閲覧のみ
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

/// 指定された権限をユーザーが持つか判定する
#[allow(dead_code)]
pub fn has_permission(caller: &Caller, perm: Permission) -> bool {
    match caller.role.as_str() {
        "admin" => true,
        "member" => matches!(perm, Permission::ViewTargets | Permission::ViewIncidents),
        _ => false,
    }
}

/// 会員がリソースの所有者であるかを確認する
#[allow(dead_code)]
pub fn is_owner(caller: &Caller, owner_id: &str) -> bool {
    caller.user_id == owner_id
}

/// 会員は自分が所有するリソースのみ閲覧可能
pub fn can_view_target(caller: &Caller, target_owner_id: &str) -> bool {
    caller.is_admin() || caller.user_id == target_owner_id
}
