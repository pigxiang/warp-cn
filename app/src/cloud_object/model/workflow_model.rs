use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
pub use cloud_object_models::{CloudWorkflow, CloudWorkflowModel, WorkflowId};

use crate::cloud_object::CloudObjectTypeAndId;
use crate::cloud_object::{
    CloudModelType, CloudObjectEventEntrypoint, CloudObjectUpsertParams, CreateCloudObjectResult,
    CreateObjectRequest, GenericServerObject, ObjectType, Revision, UpdateCloudObjectResult,
};
use crate::persistence::ModelEvent;
use crate::server::cloud_objects::update_manager::InitiatedBy;
use crate::server::ids::{ServerId, SyncId};
use crate::server::server_api::object::ObjectClient;
use crate::server::sync_queue::{QueueItem, SerializedModel};

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl CloudModelType for CloudWorkflowModel {
    type CloudObjectType = CloudWorkflow;
    type IdType = WorkflowId;

    fn model_type_name(&self) -> &'static str {
        if self.data.is_agent_mode_workflow() {
            "Prompt"
        } else {
            "Workflow"
        }
    }

    fn object_type(&self) -> ObjectType {
        ObjectType::Workflow
    }

    fn cloud_object_type_and_id(&self, id: SyncId) -> CloudObjectTypeAndId {
        CloudObjectTypeAndId::Workflow(id)
    }

    fn display_name(&self) -> String {
        self.data.name().to_string()
    }

    fn set_display_name(&mut self, name: &str) {
        self.data.set_name(name);
    }

    fn upsert_event(params: CloudObjectUpsertParams<Self>) -> ModelEvent {
        ModelEvent::UpsertWorkflow {
            workflow: CloudWorkflow::from(params),
        }
    }

    fn bulk_upsert_event(objects: Vec<CloudObjectUpsertParams<Self>>) -> ModelEvent {
        ModelEvent::UpsertWorkflows(objects.into_iter().map(CloudWorkflow::from).collect())
    }

    fn create_object_queue_item(
        &self,
        workflow: &CloudWorkflow,
        entrypoint: CloudObjectEventEntrypoint,
        initiated_by: InitiatedBy,
    ) -> Option<QueueItem> {
        if let SyncId::ClientId(client_id) = workflow.id {
            return Some(QueueItem::CreateWorkflow {
                object_type: self.object_type(),
                owner: workflow.permissions.owner,
                model: Arc::new(workflow.model().clone()),
                initial_folder_id: workflow.metadata.folder_id,
                entrypoint,
                id: client_id,
                initiated_by,
            });
        }
        None
    }

    fn update_object_queue_item(
        &self,
        revision_ts: Option<Revision>,
        workflow: &CloudWorkflow,
    ) -> QueueItem {
        QueueItem::UpdateWorkflow {
            // Note that this is intentionally a deep clone of the model because we are grabbing
            // a snapshot to update at a moment in time.
            model: workflow.model().clone().into(),
            id: workflow.id,
            revision: revision_ts.or(workflow.metadata.revision),
        }
    }

    fn should_update_after_server_conflict(&self) -> bool {
        true
    }

    fn serialized(&self) -> SerializedModel {
        SerializedModel::new(
            serde_json::to_string(&self.data).expect("failed to serialize workflow"),
        )
    }

    async fn send_create_request(
        object_client: Arc<dyn ObjectClient>,
        request: CreateObjectRequest,
    ) -> Result<CreateCloudObjectResult> {
        object_client.create_workflow(request).await
    }

    async fn send_update_request(
        &self,
        object_client: Arc<dyn ObjectClient>,
        server_id: ServerId,
        revision: Option<Revision>,
    ) -> Result<UpdateCloudObjectResult<GenericServerObject<WorkflowId, Self>>> {
        object_client
            .update_workflow(
                server_id.into(),
                serde_json::to_string(&self.data)?.into(),
                revision,
            )
            .await
    }

    fn renders_in_warp_drive(&self) -> bool {
        false
    }
}
