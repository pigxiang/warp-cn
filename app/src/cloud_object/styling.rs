use std::fmt;

use pathfinder_color::ColorU;
use warp_core::ui::appearance::Appearance;
use warp_core::ui::color::ContrastingColor;
use warp_core::ui::color::contrast::MinimumAllowedContrast;
use warp_core::ui::theme::Fill;

use crate::cloud_object::model::persistence::CloudModel;
use crate::cloud_object::{
    CloudObject, CloudObjectTypeAndId, GenericStringObjectFormat, JsonObjectType,
};
use crate::ui_components::blended_colors;
use crate::ui_components::icons::Icon;
use warpui::SingletonEntity as _;
use warpui::{AppContext, Element};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DriveObjectType {
    Workflow,
    AgentModeWorkflow,
    AIFact,
    AIFactCollection,
    Notebook {
        /// Whether the notebook was created as an AI Document (plan)
        is_ai_document: bool,
    },
    Folder,
    MCPServer,
    MCPServerCollection,
}

impl From<DriveObjectType> for Icon {
    fn from(cloud_object_type: DriveObjectType) -> Icon {
        match cloud_object_type {
            DriveObjectType::Workflow => Icon::Workflow,
            DriveObjectType::AgentModeWorkflow => Icon::Prompt,
            DriveObjectType::AIFact => Icon::BookOpen,
            DriveObjectType::AIFactCollection => Icon::BookOpen,
            DriveObjectType::Notebook { is_ai_document } => {
                if is_ai_document {
                    Icon::Compass
                } else {
                    Icon::Notebook
                }
            }
            DriveObjectType::Folder => Icon::Folder,
            DriveObjectType::MCPServer => Icon::Dataflow,
            DriveObjectType::MCPServerCollection => Icon::Dataflow,
        }
    }
}

impl fmt::Display for DriveObjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriveObjectType::Notebook { .. } => write!(f, "notebook"),
            DriveObjectType::Workflow => write!(f, "workflow"),
            DriveObjectType::Folder => write!(f, "folder"),
            DriveObjectType::AgentModeWorkflow => write!(f, "prompt"),
            DriveObjectType::AIFact => write!(f, "ai fact"),
            DriveObjectType::AIFactCollection => write!(f, "ai fact collection"),
            DriveObjectType::MCPServer => write!(f, "mcp server"),
            DriveObjectType::MCPServerCollection => write!(f, "mcp server collection"),
        }
    }
}

/// The Drive-style icon and display name for a cloud object, for surfaces that only have the
/// object's id available (e.g. AI citation chips).
pub fn cloud_object_icon_and_name(
    object: &dyn CloudObject,
    appearance: &Appearance,
    app: &AppContext,
) -> (Box<dyn Element>, String) {
    let cloud_object_type = match object.cloud_object_type_and_id() {
        CloudObjectTypeAndId::Notebook(_) => DriveObjectType::Notebook {
            is_ai_document: false,
        },
        CloudObjectTypeAndId::Workflow(id) => {
            if CloudModel::as_ref(app)
                .get_workflow(&id)
                .is_some_and(|workflow| workflow.model().data.is_agent_mode_workflow())
            {
                DriveObjectType::AgentModeWorkflow
            } else {
                DriveObjectType::Workflow
            }
        }
        CloudObjectTypeAndId::Folder(_) => DriveObjectType::Folder,
        CloudObjectTypeAndId::GenericStringObject { object_type, id: _ } => match object_type {
            GenericStringObjectFormat::Json(JsonObjectType::AIFact) => DriveObjectType::AIFact,
            GenericStringObjectFormat::Json(JsonObjectType::MCPServer)
            | GenericStringObjectFormat::Json(JsonObjectType::TemplatableMCPServer) => {
                DriveObjectType::MCPServer
            }
            GenericStringObjectFormat::Json(_) => DriveObjectType::Folder,
        },
    };

    let icon = Icon::from(cloud_object_type)
        .to_warpui_icon(Fill::from(warp_drive_icon_color(
            appearance,
            cloud_object_type,
        )))
        .finish();

    let name = match object.cloud_object_type_and_id() {
        CloudObjectTypeAndId::Notebook(id) => CloudModel::as_ref(app)
            .get_notebook(&id)
            .map(|notebook| notebook.model().title.clone()),
        CloudObjectTypeAndId::Workflow(id) => CloudModel::as_ref(app)
            .get_workflow(&id)
            .map(|workflow| workflow.model().data.name().to_owned()),
        CloudObjectTypeAndId::Folder(id) => CloudModel::as_ref(app)
            .get_folder(&id)
            .map(|folder| folder.model().name.clone()),
        CloudObjectTypeAndId::GenericStringObject { object_type, id } => match object_type {
            _ => None,
        },
    }
    .unwrap_or_else(|| object.model_type_name().to_owned());

    (icon, name)
}

pub fn warp_drive_icon_color(
    appearance: &Appearance,
    cloud_object_type: DriveObjectType,
) -> ColorU {
    match cloud_object_type {
        DriveObjectType::Workflow => {
            let color: Fill = appearance.theme().terminal_colors().normal.red.into();
            color
                .on_background(
                    appearance.theme().surface_1(),
                    MinimumAllowedContrast::NonText,
                )
                .into()
        }
        DriveObjectType::Notebook { .. } => {
            let color: Fill = appearance.theme().terminal_colors().normal.blue.into();
            color
                .on_background(
                    appearance.theme().surface_1(),
                    MinimumAllowedContrast::NonText,
                )
                .into()
        }
        DriveObjectType::Folder => {
            // Match File Tree styling - use text_sub color
            blended_colors::text_sub(appearance.theme(), appearance.theme().background())
        }
        DriveObjectType::AIFactCollection
        | DriveObjectType::AIFact
        | DriveObjectType::MCPServer
        | DriveObjectType::MCPServerCollection
        | DriveObjectType::AgentModeWorkflow => appearance
            .theme()
            .main_text_color(appearance.theme().background())
            .into(),
    }
}
