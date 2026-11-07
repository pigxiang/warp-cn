//! Content types for saved AI prompts (cloud workflow objects) and notebook command blocks.
//!
//! The Workflows feature itself has been removed; these types remain because prompts and
//! notebook commands store their content in the same workflow content model.

use cloud_object_models::{CloudWorkflow, Workflow, WorkflowId};
use serde::{Deserialize, Serialize};

use crate::cloud_object::{CloudObjectTypeAndId, Owner};
use crate::notebooks::{NotebookId, NotebookLocation};
use crate::server::ids::{ServerId, SyncId};

/// Where a piece of workflow content came from.
#[derive(Copy, Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Hash)]
pub enum WorkflowSource {
    Team {
        team_uid: ServerId,
    },
    PersonalCloud,
    WarpAI,
    Notebook {
        notebook_id: Option<NotebookId>,
        team_uid: Option<ServerId>,
        location: NotebookLocation,
    },
}

/// How the user reached a piece of workflow content.
#[derive(Copy, Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Hash, PartialOrd)]
pub enum WorkflowSelectionSource {
    UniversalSearch,
    WarpAI,
    Notebook,
    SlashMenu,
    UpArrowHistory,
    AgentMode,
    Undefined,
}

impl From<Owner> for WorkflowSource {
    fn from(owner: Owner) -> WorkflowSource {
        match owner {
            Owner::User { .. } => Self::PersonalCloud,
            Owner::Team { team_uid } => Self::Team { team_uid },
        }
    }
}

/// Wrapper type for workflow content that may come from a cloud-synced prompt object or
/// from a notebook command block.
#[derive(Clone, Debug, PartialEq)]
pub enum WorkflowType {
    /// Saved prompts from personal or team collections, saved using cloud sync.
    Cloud(Box<CloudWorkflow>),
    /// Workflow content that's part of a cloud notebook.
    Notebook(Workflow),
}

impl WorkflowType {
    pub fn as_workflow(&self) -> &Workflow {
        match self {
            WorkflowType::Cloud(workflow) => &workflow.model().data,
            WorkflowType::Notebook(workflow) => workflow,
        }
    }

    /// Returns the contained [`Workflow`], consuming `self`.
    pub fn take_workflow(self) -> Workflow {
        match self {
            WorkflowType::Cloud(workflow) => workflow.model().data.clone(),
            WorkflowType::Notebook(workflow) => workflow,
        }
    }

    /// The object type and ID for the cloud object containing this content, if there is one.
    /// This is only supported for cloud prompts, not content within notebooks.
    pub fn object_id(&self) -> Option<CloudObjectTypeAndId> {
        match self {
            WorkflowType::Cloud(workflow) => Some(CloudObjectTypeAndId::Workflow(workflow.id)),
            _ => None,
        }
    }

    pub fn sync_id(&self) -> Option<SyncId> {
        match self {
            WorkflowType::Cloud(workflow) => Some(workflow.id),
            _ => None,
        }
    }

    pub fn server_id(&self) -> Option<WorkflowId> {
        match self.object_id() {
            Some(CloudObjectTypeAndId::Workflow(id)) => id.into_server().map(Into::into),
            _ => None,
        }
    }
}
