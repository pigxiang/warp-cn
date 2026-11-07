use std::sync::Arc;

use chrono::Utc;
use cloud_object_client::MockObjectClient;
use settings::manager::SettingsManager;
use warp_graphql::object_permissions::AccessLevel;
use warpui::{App, SingletonEntity, WindowId};

use super::*;
use crate::auth::{AuthStateProvider, UserUid};
use crate::cloud_object::model::persistence::{CloudModel, UpdateSource};
use crate::cloud_object::model::view::CloudViewModel;
use crate::cloud_object::{CloudWorkflowModel, WorkflowId};
use crate::cloud_object::{
    Owner, Revision, ServerGuestSubject, ServerMetadata, ServerObjectGuest, ServerPermissions,
    ServerWorkflow,
};
use crate::features::FeatureFlag;
use crate::network::NetworkStatus;
use crate::search::data_source::Query;
use crate::server::cloud_objects::update_manager::UpdateManager;
use crate::server::ids::SyncId::{self};
use crate::server::ids::{ObjectUid, ServerId};
use crate::server::server_api::ServerApiProvider;
use crate::server::server_api::team::MockTeamClient;
use crate::server::server_api::workspace::MockWorkspaceClient;
use crate::server::sync_queue::SyncQueue;
use crate::settings::{AISettings, PrivacySettings};
use crate::system::SystemStats;
use crate::test_util::assert_eventually;
use crate::workspaces::team::{Team, TeamVisibility};
use crate::workspaces::team_tester::TeamTesterStatus;
use crate::workspaces::user_profiles::UserProfiles;
use crate::workspaces::user_workspaces::UserWorkspaces;
use crate::workspaces::workspace::Workspace;
use cloud_object_models::Workflow;

fn mock_server_metadata() -> ServerMetadata {
    ServerMetadata {
        uid: ServerId::default(),
        revision: Revision::now(),
        metadata_last_updated_ts: Utc::now().into(),
        trashed_ts: None,
        folder_id: None,
        is_welcome_object: false,
        creator_uid: None,
        last_editor_uid: None,
        current_editor_uid: None,
    }
}

fn mock_server_permissions(owner: Owner) -> ServerPermissions {
    ServerPermissions {
        space: owner,
        guests: Vec::new(),
        anyone_link_sharing: None,
        permissions_last_updated_ts: Utc::now().into(),
    }
}

fn mock_server_workflow(id: WorkflowId, owner: Owner) -> ServerWorkflow {
    mock_named_server_workflow(id, owner, format!("foo{id}"), format!("bar{id}"))
}

fn mock_named_server_workflow(
    id: WorkflowId,
    owner: Owner,
    name: impl Into<String>,
    command: impl Into<String>,
) -> ServerWorkflow {
    ServerWorkflow::new(
        SyncId::ServerId(id.into()),
        CloudWorkflowModel::new(Workflow::new(name, command)),
        mock_server_metadata(),
        mock_server_permissions(owner),
    )
}

fn team_for_test(uid: i64, name: &str) -> Team {
    Team {
        uid: uid.into(),
        name: name.to_owned(),
        color: None,
        invite_link: None,
        members: vec![],
        pending_email_invites: vec![],
        invite_link_domain_restrictions: vec![],
        billing_metadata: Default::default(),
        stripe_customer_id: None,
        settings: Default::default(),
        feature_model_choice: Default::default(),
        is_eligible_for_discovery: false,
        has_billing_history: false,
        visibility: TeamVisibility::Open,
    }
}

fn workspace_for_test(teams: Vec<Team>) -> Workspace {
    Workspace {
        uid: "workspace_uid123456789".to_string().into(),
        name: "test".to_string(),
        stripe_customer_id: None,
        teams,
        open_teams: vec![],
        billing_metadata: Default::default(),
        bonus_grants_purchased_this_month: Default::default(),
        billing_cycle_usage: None,
        has_billing_history: false,
        settings: Default::default(),
        feature_model_choice: Default::default(),
        invite_link_domain_restrictions: vec![],
        pending_email_invites: vec![],
        is_eligible_for_discovery: false,
        members: vec![],
        total_requests_used_since_last_refresh: 0,
    }
}

fn initialize_app(app: &mut App, workspaces: Vec<Workspace>) {
    // Add the necessary singleton models to the App
    app.add_singleton_model(|_| NetworkStatus::new());
    app.add_singleton_model(|_| SystemStats::new());
    let mock_team_client = Arc::new(MockTeamClient::new());
    let mock_workspace_client = Arc::new(MockWorkspaceClient::new());
    app.add_singleton_model(|ctx| {
        UserWorkspaces::mock(
            mock_team_client.clone(),
            mock_workspace_client.clone(),
            workspaces,
            ctx,
        )
    });
    app.add_singleton_model(TeamTesterStatus::new);
    // `update_workspaces` pushes enterprise settings into PrivacySettings.
    app.add_singleton_model(PrivacySettings::mock);
    app.add_singleton_model(SyncQueue::mock);
    app.add_singleton_model(CloudModel::mock);
    app.add_singleton_model(|ctx| UpdateManager::new(None, Arc::new(MockObjectClient::new()), ctx));
    app.add_singleton_model(|_| UserProfiles::new(Vec::new()));
    app.add_singleton_model(CloudViewModel::new);
    app.add_singleton_model(|_| ServerApiProvider::new_for_test());
    app.add_singleton_model(|_| SettingsManager::default());
    app.add_singleton_model(|_| AuthStateProvider::new_for_test());
    app.update(crate::settings::init_and_register_user_preferences);
    app.update(AISettings::register_and_subscribe_to_events);
}

/// First id reserved for index markers, kept clear of the ids the tests assert on.
const INDEX_MARKER_ID: i64 = 900;

fn workflow_labels(
    mixer: &ModelHandle<CommandPaletteMixer>,
    query: &str,
    app: &mut App,
) -> Vec<String> {
    mixer.update(app, |mixer, ctx| {
        mixer.run_query(
            Query {
                filters: HashSet::from([QueryFilter::Workflows]),
                text: query.into(),
            },
            ctx,
        );
    });
    app.read(|app| {
        let mut labels = mixer
            .as_ref(app)
            .results()
            .iter()
            .map(|result| result.accessibility_label())
            .collect::<Vec<_>>();
        labels.sort();
        labels
    })
}

fn workflow_label(name: &str) -> String {
    format!("Workflow: {name}")
}

/// Polls the palette until it reports `expected`, so tests synchronise on the background indexer
/// instead of racing a fixed delay.
async fn assert_workflow_labels_eventually(
    mixer: &ModelHandle<CommandPaletteMixer>,
    query: &str,
    expected: &[String],
    app: &mut App,
) {
    assert_eventually!(
        2000 => workflow_labels(mixer, query, app) == expected,
        "workflow labels for {query:?} did not match after ~10s. last observed: {:?}, expected: {expected:?}",
        workflow_labels(mixer, query, app)
    );
}

/// Indexes a fresh in-scope workflow and waits for it to become searchable.
///
/// The searcher drains its queue in order, so once the marker is visible every operation queued
/// before it has been applied. Without this, asserting that something is *absent* from the index
/// would pass simply because the indexer had not run yet.
async fn drain_index(marker_id: i64, mixer: &ModelHandle<CommandPaletteMixer>, app: &mut App) {
    let marker_name = format!("indexmarker{marker_id}");
    CloudModel::handle(app).update(app, |model, ctx| {
        model.upsert_from_server_workflow(
            mock_named_server_workflow(
                marker_id.into(),
                Owner::mock_current_user(),
                marker_name.clone(),
                "echo marker",
            ),
            ctx,
        );
    });
    assert_workflow_labels_eventually(mixer, &marker_name, &[workflow_label(&marker_name)], app)
        .await;
}

fn prompt_or_workflow_uid(id: i64) -> ObjectUid {
    SyncId::ServerId(WorkflowId::from(id).into()).uid()
}

/// Enough out-of-window workflows to more than fill the full-text searcher's result cap. They must
/// never reach the ranker: they are not in this window's corpus at all.
const CROWDING_WORKFLOW_COUNT: i64 = 25;

fn mock_shared_server_permissions(shared_with: UserUid) -> ServerPermissions {
    ServerPermissions {
        space: Owner::User {
            user_uid: UserUid::new("someone-else"),
        },
        guests: vec![ServerObjectGuest {
            subject: ServerGuestSubject::User {
                firebase_uid: shared_with.as_string(),
            },
            access_level: AccessLevel::Viewer,
            source: None,
        }],
        anyone_link_sharing: None,
        permissions_last_updated_ts: Utc::now().into(),
    }
}

fn current_user_uid(app: &App) -> UserUid {
    app.read(|app| {
        AuthStateProvider::as_ref(app)
            .get()
            .user_id()
            .expect("test user should be authenticated")
    })
}
