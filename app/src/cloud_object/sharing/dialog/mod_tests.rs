use warpui::{App, SingletonEntity, TypedActionView, ViewHandle};

use super::{SharingDialog, SharingDialogAction};
use crate::auth::UserUid;
use crate::cloud_object::Owner;
use crate::cloud_object::model::persistence::CloudModel;
use crate::cloud_object::model::view::CloudViewModel;
use crate::cloud_object::sharing::{ShareableObject, SharingAccessLevel};
use crate::cloud_object::{CloudWorkflow, CloudWorkflowModel};
use crate::server::ids::{ClientId, ServerId, SyncId};
use crate::test_util::terminal::{
    add_window_with_id_and_terminal, initialize_app_for_terminal_view,
};
use crate::workspaces::team::{Team, TeamVisibility};
use crate::workspaces::user_profiles::UserProfiles;
use crate::workspaces::user_workspaces::UserWorkspaces;
use crate::workspaces::workspace::{
    EnforceableSetting, TeamLinkSharingSettings, TeamSettings, Workspace, WorkspaceSettings,
};
use cloud_object_models::Workflow;

fn team_with_link_sharing(uid: i64, name: &str, permitted: bool) -> Team {
    let permission = EnforceableSetting {
        value: permitted,
        is_enforced_by_workspace: false,
    };
    Team {
        uid: uid.into(),
        name: name.to_string(),
        color: None,
        invite_link: None,
        members: vec![],
        pending_email_invites: vec![],
        invite_link_domain_restrictions: vec![],
        billing_metadata: Default::default(),
        stripe_customer_id: None,
        settings: TeamSettings {
            link_sharing: TeamLinkSharingSettings {
                anyone_with_link_sharing_enabled: permission.clone(),
                direct_link_sharing_enabled: permission,
            },
            ..Default::default()
        },
        feature_model_choice: Default::default(),
        is_eligible_for_discovery: false,
        has_billing_history: false,
        visibility: TeamVisibility::Open,
    }
}

fn install_workspace_with_teams(app: &mut App, teams: Vec<Team>) {
    let workspace = Workspace {
        uid: "workspace_uid123456789".to_string().into(),
        name: "test".to_string(),
        stripe_customer_id: None,
        teams,
        open_teams: vec![],
        billing_metadata: Default::default(),
        bonus_grants_purchased_this_month: Default::default(),
        billing_cycle_usage: None,
        has_billing_history: false,
        settings: WorkspaceSettings::default(),
        feature_model_choice: Default::default(),
        invite_link_domain_restrictions: vec![],
        pending_email_invites: vec![],
        is_eligible_for_discovery: false,
        members: vec![],
        total_requests_used_since_last_refresh: 0,
    };
    let workspace_uid = workspace.uid;

    let user_workspaces = UserWorkspaces::handle(&*app);
    user_workspaces.update(app, |user_workspaces, ctx| {
        user_workspaces.update_workspaces(vec![workspace], ctx);
        user_workspaces.set_current_workspace_uid(workspace_uid, ctx);
    });
}

#[test]
fn link_sharing_gates_follow_a_window_onto_its_new_team() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);

        let (window_id, _terminal) = add_window_with_id_and_terminal(&mut app, None);

        let permitted_team = team_with_link_sharing(123, "permits-sharing", true);
        let forbidden_team = team_with_link_sharing(456, "forbids-sharing", false);
        install_workspace_with_teams(
            &mut app,
            vec![permitted_team.clone(), forbidden_team.clone()],
        );

        let user_workspaces = UserWorkspaces::handle(&app);
        user_workspaces.update(&mut app, |user_workspaces, ctx| {
            user_workspaces.set_team_for_window(window_id, permitted_team.uid, ctx);
        });

        let dialog = app.add_typed_action_view(window_id, |ctx| SharingDialog::new(None, ctx));
        dialog.read(&app, |dialog, ctx| {
            assert!(dialog.can_anyone_with_link_share(ctx));
            assert!(dialog.can_direct_link_share(ctx));
        });

        install_workspace_with_teams(&mut app, vec![forbidden_team]);

        dialog.read(&app, |dialog, ctx| {
            assert!(!dialog.can_anyone_with_link_share(ctx));
            assert!(!dialog.can_direct_link_share(ctx));
        });
    });
}

fn initialize_app_for_drive_object_dialog(app: &mut App) {
    initialize_app_for_terminal_view(app);
    app.add_singleton_model(CloudViewModel::mock);
    app.add_singleton_model(|_| UserProfiles::new(Vec::new()));
}

fn add_shareable_object(app: &mut App) -> ServerId {
    let object_uid: ServerId = 789.into();
    let mut object = CloudWorkflow::new_local(
        CloudWorkflowModel {
            data: Workflow::new("shared workflow", "echo shared"),
        },
        Owner::User {
            user_uid: UserUid::new("owner"),
        },
        None,
        ClientId::default(),
    );
    object.id = SyncId::ServerId(object_uid);

    let cloud_model = CloudModel::handle(&*app);
    cloud_model.update(app, |cloud_model, _| {
        cloud_model.add_object(object.id, object);
    });
    object_uid
}

fn permissions_change_reached_object(app: &App, object_uid: ServerId) -> bool {
    app.read(|ctx| {
        CloudModel::as_ref(ctx)
            .get_by_uid(&object_uid.uid())
            .expect("the targeted object should be in the cloud model")
            .metadata()
            .pending_changes_statuses
            .has_pending_permissions_change
    })
}

fn set_link_permissions(
    dialog: &ViewHandle<SharingDialog>,
    access_level: Option<SharingAccessLevel>,
    app: &mut App,
) {
    dialog.update(app, |dialog, ctx| {
        dialog.handle_action(&SharingDialogAction::SetLinkPermissions(access_level), ctx);
    });
}

fn assert_set_link_permissions_dispatch(
    permitted: bool,
    access_level: Option<SharingAccessLevel>,
    expected: bool,
) {
    App::test((), |mut app| async move {
        initialize_app_for_drive_object_dialog(&mut app);

        let (window_id, _terminal) = add_window_with_id_and_terminal(&mut app, None);
        let team = team_with_link_sharing(123, "team", permitted);
        install_workspace_with_teams(&mut app, vec![team.clone()]);

        UserWorkspaces::handle(&app).update(&mut app, |user_workspaces, ctx| {
            user_workspaces.set_team_for_window(window_id, team.uid, ctx);
        });

        let object_uid = add_shareable_object(&mut app);
        let dialog = app.add_typed_action_view(window_id, |ctx| {
            SharingDialog::new(Some(ShareableObject::WarpDriveObject(object_uid)), ctx)
        });

        set_link_permissions(&dialog, access_level, &mut app);
        assert_eq!(
            permissions_change_reached_object(&app, object_uid),
            expected
        );
    });
}

#[test]
fn set_link_permissions_refuses_to_grant_under_a_forbidding_team() {
    assert_set_link_permissions_dispatch(false, Some(SharingAccessLevel::View), false);
}

#[test]
fn set_link_permissions_allows_revocation_under_a_forbidding_team() {
    assert_set_link_permissions_dispatch(false, None, true);
}

#[test]
fn set_link_permissions_grants_under_a_permitting_team() {
    assert_set_link_permissions_dispatch(true, Some(SharingAccessLevel::View), true);
}
