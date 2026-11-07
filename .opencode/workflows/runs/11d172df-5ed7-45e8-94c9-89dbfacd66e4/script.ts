import { defineWorkflow, z } from "@malhashemi/opencode-dynamic-workflows/workflow"

const Report = z.object({
  files_changed: z.array(z.string()),
  decisions: z.array(z.string()),
  left_over: z.array(z.string()),
})

const CONVENTIONS = `
背景：仓库 /Users/xiang/code/rust/warp-cn（分支 lite）正在"物理删除 Warp Drive 与 Workflows 功能"。app/src/drive 与 app/src/workflows 目录已删除。禁止 stub/todo/占位；因删除而失效的字段/参数/方法/枚举变体/match 臂/导入要连带删除（未用参数直接删，不用 _ 前缀）。保持 AGENTS.md Rust 风格（穷尽 match、少注释、内联格式参数）。

已迁移的模块（这些 import 路径已存在，旧路径 crate::drive::… / crate::workflows::… 不存在）：
- crate::cloud_object::{CloudObjectTypeAndId}（从 crates cloud_objects 再导出）
- crate::cloud_object::sharing::{ContentEditability, ShareableObject, SubjectExt, …}（原 drive/sharing，整体保留，会话/对话分享功能保留）
- crate::cloud_object::export::ExportManager（原 drive/export；workflow 导出分支已删）
- crate::cloud_object::drive_helpers::{is_feature_gated_anonymous_user_past_env_var_limit 等}（原 drive/drive_helpers；workflow 变体已删）
- crate::cloud_object::styling::{DriveObjectType, warp_drive_icon_color}（原 drive/mod + cloud_object_styling）
- crate::settings::warp_drive_settings::{WarpDriveSettings, DriveSortOrder}（原 drive/settings + DriveSortOrder；本波保留该 settings 组与注册，B 波再删）
- crate::cloud_object::model::folder_model::{CloudFolder, CloudFolderModel, FolderId}（原 drive/folders；folder 同步本波保留，to_warp_drive_item 成员已删）
- crate::cloud_object::model::workflow_model::{CloudWorkflow, CloudWorkflowModel, WorkflowId}（原 app/src/workflows/mod 的 CloudModelType impl，为 AI saved prompts 云同步保留；renders_in_warp_drive=false，to_warp_drive_item 已删；upsert_event 仍用 ModelEvent::UpsertWorkflow/UpsertWorkflows，这些变体保留）
- crate::server::server_api::generated_command_metadata::{GeneratedCommandMetadata,…}
- 原 crate::workflows::workflow::{Workflow, Argument, ArgumentType} 与 crate::workflows::{CloudWorkflow, CloudWorkflowModel, WorkflowId} 这些是 crates/cloud_object_models 的 re-export：需要时直接 use cloud_object_models::{…}。
- 已彻底删除的类型（引用处全部删掉其代码路径）：WorkflowType / WorkflowSource / WorkflowSelectionSource / WorkflowViewMode / AIWorkflowOrigin / CategoriesView / WorkflowView / WorkflowModal / WorkflowManager / LocalWorkflows / WorkflowAliases / ArgumentsState / WorkflowsInfoBox / DrivePanel / DriveIndex / ImportModal / WarpDriveItem / WarpDriveItemId / OpenWarpDriveObject{Args,Settings} / NewWorkspaceSource::WorkflowById / LeafContents::Workflow / WorkflowPaneSnapshot / ModalSizes.warp_drive_index_width / ModalType::WarpDriveIndexWidth。
- CloudModel 上已删除的方法：overwrite_workflow(s?)、active_workflows_in_space、active_non_welcome_workflows_in_space、workflow_enums_with_owner、collapse_all_in_location、force_expand_object_and_ancestors_cloud_id、upsert_from_server_workflow 仍保留（saved prompts 需要），get_workflow/get_workflow_by_uid/get_workflow_mut/get_all_active_workflows 已恢复保留。Input 上已删除：show_workflows_info_box_on_workflow_selection、show_workflow_info_box_for_history_command、is_workflows_info_box_open、workflows_info_box_open_workflow_cloud_id、close_voltron、is_voltron_open、voltron_view、workflows_state。
- InputAction::SelectAndRefreshVoltron / VoltronItem 已删。CommandSearchItemAction::AcceptWorkflow / AcceptedWorkflow / OpenWarpAI / TranslateUsingWarpAI 已删。
- 保留不动：crates/*（含 cloud_object_models、cloud_objects、persistence schema.rs/migrations）、FeatureFlag 枚举、QueryFilter 枚举（warp_search_core crate 里的 Workflows/NaturalLanguage 变体保留，只是 app 不再产生）、telemetry events.rs 里 workflow/drive 事件变体保留（只是不再发送）、app/src/cloud_object 其余、env_vars、notebooks、shared_session、ai facts/mcp 产品功能、AI saved prompts（slash 菜单 /prompts，数据模型仍用 CloudWorkflow，注意保留其功能）。
- Voltron 整体删除（它唯一的功能就是 Workflows）：删 app/src/voltron.rs、lib.rs 的 mod voltron（如有）、input/{agent,classic,terminal,universal}.rs 里剩余 voltron 引用、terminal/view.rs 的 is_voltron_open()/VoltronActive context、persistence/sqlite + app_state + resizable_data 里 voltron_width（crates/persistence 的 model 字段 voltron_width 也删，DB 列不动）。
- WorkspaceAction / workspace::Event 里 drive/workflow 变体（OpenWarpDrive、ViewObjectInWarpDrive、UndoTrash、OpenWorkflowModal*、ShowWorkflows 等）一律删除，所有派发处删除。
- pane 持久化：LeafContents::Workflow、WORKFLOW_PANE_KIND、workflow_panes 表 Rust 侧（crates/persistence/src/model.rs 的 WorkflowPane/NewWorkflowPane 结构删除；schema.rs 不动）。
- history 的 LinkedWorkflowData、cloud_workflow_id/workflow_command Rust 字段已删（crates/persistence/src/model.rs 中 Command/CommandMetadata 结构体的 cloud_workflow_id/workflow_command 字段也要删；schema 列不动）。

工作方式：优先用 grep 定位你负责文件里的 crate::drive / crate::workflows / DrivePanel / WorkflowModal / Workflow(… 等引用并删除对应代码路径；不要 git add/commit；不要改动不属于你清单的文件；不要运行 cargo（由主控统一编译）。若某函数删空后只剩空壳，删函数及其调用点。
汇报格式（简短）：files_changed / decisions（拿不准的处置）/ left_over（你建议留给我处理的）。
`

export default defineWorkflow({
  meta: {
    name: "wave2a-file-clusters",
    description: "Purge Drive/Workflows references from four disjoint file clusters",
    phases: [{ title: "purge" }],
  },
  async run({ agent, parallel }) {
    const tasks = [
      {
        label: "A-workspace",
        prompt: `${CONVENTIONS}
你的文件清单（只允许改这些）：
app/src/workspace/view.rs, app/src/workspace/mod.rs, app/src/workspace/action.rs, app/src/workspace/view/left_panel.rs, app/src/workspace/view/right_panel.rs, app/src/workspace/view/vertical_tabs.rs, app/src/workspace/view/conversation_list/item.rs, app/src/workspace/view/conversation_list/view.rs, app/src/workspace/view_tests.rs, app/src/workspace/view_tests 无关不要动其他文件。
workspace/view.rs 重点（行号可能漂移，先 grep）：
- 删除 DrivePanel 字段/构建/订阅/事件处理（DrivePanelEvent match 全删）、ImportModal 字段+handler、WorkflowModal 字段+open_workflow_modal/run_workflow_in_active_input/insert workflow 相关全部方法与调用点、OpenWarpDriveObject*、has_warp_drive_initialized_sections、open_workflow_from_intent、view_in_and_focus_warp_drive、update_warp_drive_view、ViewObjectInWarpDrive/UndoTrash/OpenWarpDrive action 与 handler、WarpDriveItemId 用法（连同所在 if/分支删除）。
- RunAISuggestedCommand(code) 改为与 RunCommand 相同行为：insert_in_input(command.trim(), true, true, false, ctx)。
- 11340-11350 附近 workflows info box 检查、16480-16560 与 17780-18000 与 18090-18160 的 warp drive/workflow 选择分支、18893 附近以 WarpDriveItemId 为参数的方法、4140-4260 NewWorkspaceSource::WorkflowById 臂、17880 附近 tail_command_for_shell、11800-11845 WindowSnapshot 构造里的 warp_drive_index_width 与 voltron_width 字段（这两个 width 字段已从 WindowSnapshot 删除；voltron 也整体删除）。
- workspace/mod.rs：with_custom_action(NewTeamWorkflow/NewPersonalWorkflow/ToggleWarpDrive/SearchDrive/NewPersonalAIPrompt/NewTeamAIPrompt) 与 BindingGroup::Workflow 映射行删除；CustomAction::Workflows 相关删除。注意保留 EnvVars/AIFact/MCPServer/OpenTeamSettings/Share* 的 custom action 映射。
- action.rs：删 drive/workflow 变体（OpenWarpDrive、ViewObjectInWarpDrive(CloudObjectTypeAndId?)、UndoTrash、OpenWorkflowModal… 等）与 WorkflowSelectionSource/WorkflowSource/WorkflowType 导入；CloudObjectTypeAndId 改用 crate::cloud_object::CloudObjectTypeAndId（若某 action 仍需要它且功能保留则保留）。
- left_panel.rs：删 DrivePanel tab/入口（保留其它 tab）；right_panel.rs 删 drive panel 宽度/常量引用；vertical_tabs.rs 删 DriveObjectType/cloud_object_styling 引用处（若仅图标着色则改用 crate::cloud_object::styling）。
- conversation_list/{item,view}.rs：SharingDialog 引用改 crate::cloud_object::sharing::dialog::SharingDialog（功能保留！分享会话/对话保留）。
- view_tests.rs：删除引用 drive/workflow 的测试（LocalWorkflows、WorkflowView 等）。`,
      },
      {
        label: "B-notebooks-panegroup",
        prompt: `${CONVENTIONS}
你的文件清单：
app/src/notebooks/mod.rs, app/src/notebooks/link.rs, app/src/notebooks/telemetry.rs, app/src/notebooks/editor/mod.rs, app/src/notebooks/editor/embedded_item.rs, app/src/notebooks/editor/embedding_model.rs, app/src/notebooks/editor/notebook_command.rs, app/src/notebooks/editor/block_insertion_menu.rs, app/src/notebooks/editor/model_tests.rs, app/src/notebooks/file/mod.rs,
app/src/pane_group/mod.rs, app/src/pane_group/mod_tests.rs, app/src/pane_group/pane/mod.rs, app/src/pane_group/pane/file_pane.rs, app/src/pane_group/pane/env_var_collection_pane.rs, app/src/pane_group/pane/view/header/sharing.rs, app/src/pane_group/pane/view/mod.rs（如引用）。
要点：
- pane 枚举 Workflow 变体、workflow_pane 一切引用、Pane::Workflow match 臂、workflow pane 持久化恢复（WORKFLOW_PANE_KIND 相关）全部删除；PaneType/pane kind 字符串映射删除。
- notebooks：workflow 嵌入功能删除（embedded_item 的 workflow 变体与相关渲染、embedding_model 的 workflow 数据源、notebook_command 的 ArgumentsState/workflow 参数路径、editor/mod 的 WorkflowSource/WorkflowType）。CloudWorkflow/WorkflowId 若是 crate 类型且仍服务非 workflow 嵌入（大概率不需要），直接删功能。
- notebooks/mod.rs WarpDriveItem trait impl 已删成员（to_warp_drive_item）——残留 import 删除。notebooks/link.rs：drive uri link 功能删除（OpenWarpDriveObjectArgs）。telemetry.rs 的 WorkflowId 导入删除（如字段仅服务 workflow）。
- notebooks 保留 CloudNotebook 与 cloud_object 胶水（这是本波保留项）。
- pane header sharing.rs：保留！SharingDialog 改 import crate::cloud_object::sharing::dialog::{SharingDialog, SharingDialogEvent}；ShareableObject/ContentEditability 改 crate::cloud_object::sharing。
- env_var_collection_pane.rs：WarpDriveItemId 引用（ViewObjectInWarpDrive action）删除该功能分支。
- file_pane.rs：WorkflowSelectionSource 引用删除（workflow 打开 notebook 的入口）。
- model_tests.rs / mod_tests.rs：删除 drive/workflow 测试与 fixture。`,
      },
      {
        label: "C-terminal-voltron-integration",
        prompt: `${CONVENTIONS}
你的文件清单：
app/src/voltron.rs（整个删除：git rm），app/src/lib.rs 中若有 mod voltron 声明删除，
app/src/terminal/view.rs, app/src/terminal/view/action.rs, app/src/terminal/block_list_element.rs, app/src/terminal/view/block_onboarding/mod.rs（onboarding_drive_sharing_block 注册删除；文件已删）, app/src/terminal/view/block_onboarding/onboarding_drive_sharing_block.rs（若还在则 git rm）, app/src/terminal/view/context_menu.rs, app/src/terminal/view/pane_impl.rs, app/src/terminal/view/shared_session/view_impl.rs, app/src/terminal/view/shared_session/cloud_conversation_continuation.rs, app/src/terminal/input/（全部剩余文件里 voltron 残留）, app/src/terminal/input.rs（少量剩余错误）, app/src/terminal/tui_export.rs（LinkedWorkflowData），
app/src/integration_testing/mod.rs（删 pub mod warp_drive; pub mod workflow;），app/src/integration_testing/assertions.rs, view_getters.rs, type_getters.rs, keybindings.rs, launch_configs.rs（引用 drive/workflow 处）,
crates/integration/src/test.rs（mod workflows 声明、test_add_workflows_to_warp_config 等 workflow/drive 测试函数、CategoriesView 导入、integration_testing::workflow 引用全部删除）, crates/integration/src/**（任何 drive/workflows 引用，含 test/session_restoration.rs 的 warp_drive 断言）。
要点：
- terminal/view.rs：删 "Save as workflow" block 右键菜单项与 handler、OpenWorkflowModal* action（view/action.rs Event/BlocklistAction? 变体删）、VoltronActive context 与 is_voltron_open、drive settings 相关（WarpDriveSettings import 改 crate::settings::warp_drive_settings；调用 is_warp_drive_enabled 的分支若为 drive 入口则删）、Event::ExecuteCommand 处理里 workflow_id/workflow_command（ExecuteCommandEvent 字段已删，见 view.rs:1703 结构体也要删字段与 handler 用法）、history.add for_session_command 调用去掉 workflow 参数。
- block_list_element.rs / context_menu：drive/workflow 菜单项删。
- tui_export.rs：LinkedWorkflowData 引用删除。
- integration：删 workflow 测试与 view getter（CategoriesView、WorkflowView getter、drive panel getter）。保留其余。
- voltron 剩余：terminal/view.rs / resizable? 不，resizable_data 和 app_state 由别人负责——如发现你文件里引用 ModalType::VoltronWidth/ModalSizes.voltron_width/ResizableData 请列入 left_over。`,
      },
      {
        label: "D-persistence-settings-search-ai-env",
        prompt: `${CONVENTIONS}
你的文件清单：
app/src/persistence/mod.rs, app/src/persistence/sqlite.rs, crates/persistence/src/model.rs（删 WorkflowPane/NewWorkflowPane/WORKFLOW_PANE_KIND/Command & NewCommand 结构体里 cloud_workflow_id/workflow_command 字段；schema.rs 不动）,
app/src/app_state.rs（删 voltron_width：WindowSnapshot 字段与 ModalSizes 无关；app_state 的 WindowSnapshot.voltron_width 字段删除）、app/src/terminal/resizable_data.rs（删 VoltronWidth/DEFAULT_VOLTRON_WIDTH/voltron_width 字段与 handle）、app/src/persistence/sqlite_tests.rs（voltron_width 构造删除）,
app/src/auth/mod.rs（删 WorkflowManager 导入与 59 行附近 workflow manager 调用；CloudModel 未保存对象警告逻辑保留——它用 crate cloud_object；drive 引用改 crate::cloud_object），app/src/settings/onboarding.rs + onboarding_tests.rs（删 WarpDriveSettings 写入步骤与断言——import 已改 crate::settings::warp_drive_settings，仅删对 enable_warp_drive 的写入/断言代码），app/src/settings/init.rs（保留 WarpDriveSettings::register），app/src/settings/cloud_preferences_syncer.rs（import 已改，检查剩余）,
app/src/settings_view/mod.rs（删 SettingsSection::WarpDrive 变体及全部臂：nav 注册 1493、page 1384、事件 1970、Section 名 453、383；删 WarpDriveSettingsPage* 引用）, app/src/settings_view/teams_page.rs（cloud_action_confirmation_dialog 已删——找到使用处删除或替换为通用确认对话框？直接删该功能分支）, app/src/settings_view/appearance_page.rs（删 "Show Warp Drive tab in tools panel" 行与 ToggleToolsPanelWarpDrive action）、app/src/settings_view/knowledge_page.rs 与 environments_page.rs 与 mcp_servers/list_page.rs（drive/workflow 引用处评估删除入口）,
app/src/search/**（action/search_item.rs 的 BindingGroup::Workflow 臂；command_palette/filter_chip_renderer.rs 删 Workflows/AgentModeWorkflows/Drive chip 臂与 QueryFilter::Drive（若有 app 本地）；command_palette/{data_sources,mixer,view,zero_state}.rs 删 warp_drive 数据源注册与 Workflows filter；command_search/history/history_search_item.rs 删 linked workflow；search/notebook_embedding/{view.rs,notebooks/notebook_search_item.rs} 删 workflows 子模块引用与 drive import；ai_context_menu/view.rs 删 Workflows category 臂；search/mod.rs 剩余）,
app/src/ai/**（ai_assistant/mod.rs、agent_management/{view.rs,cloud_setup_guide_view.rs}、agent_sdk/mod.rs、blocklist/{suggested_agent_mode_workflow_modal.rs（若整文件仅 workflow 创建弹窗则 git rm 并删注册点——注册点若在他文件列 left_over}, blocklist/suggestion_chip_view.rs, blocklist/inline_action/requested_command_attribution.rs, document/ai_document_model.rs（CloudFolder import 改 cloud_object_models）, facts/{mod.rs,view/{mod.rs,rule.rs}}（to_warp_drive_item/WarpDriveAIFact 已删，清残留）、mcp/{mod.rs,templatable.rs,templatable_manager/native.rs}（WarpDriveMCPServer 已删，清残留）, ai_document_view.rs（WarpDriveItemId/CreateWarpDriveNotebook action 与 create_warp_drive_notebook：drive 打开功能删除）, ambient_agents/scheduled.rs, execution_profiles/profiles.rs, facts、mcp 的 drive::CloudObjectTypeAndId import 已改路径则跳过）,
app/src/env_vars/**（menus.rs：ExportManager import 改 crate::cloud_object::export、drive_helpers 改 crate::cloud_object::drive_helpers、has_feature_gated_anonymous_user_reached_env_var_limit 保留（功能保留）、CloudObjectTypeAndId 改 crate::cloud_object；env_var_collection.rs/mod.rs：WarpDriveItem/impl 残留删除；active_env_var_collection_data.rs/secrets.rs/fixed_view_components.rs：crate::cloud_object::sharing 已改路径）,
app/src/local_control/handlers/metadata.rs（SurfaceDestination::WarpDrive 处理删除；enum 本体如仅此处用则列 left_over）,
app/src/server/**（update_manager.rs：WorkflowManager/CloudWorkflow import 处理——saved prompts 的 sync 保留，drive-only 删除；sync_queue.rs、server_api/object.rs（Workflow import 改 cloud_object_models 或 crate）、telemetry/events.rs（CloudObjectTypeAndId import 改 crate::cloud_object::CloudObjectTypeAndId、DriveSortOrder 改 crate::settings::warp_drive_settings）；ids_tests/sync_queue_tests/fake_object_client/update_manager_tests：drive import 改路径或删 workflow-only 测试）,
app/src/cloud_object/mod.rs（补回 use self::breadcrumbs::ContainingObject;）, app/src/cloud_object/model/{view.rs, generic_string_model.rs, model_tests.rs}（folders import 已改 cloud_object_models；model_tests 删 workflow enum/文件夹 welcome 测试？保留 folder 测试）, app/src/completer/mod.rs（SessionContext::new 的 ctx 参数若已不用：删参数并更新所有调用点——调用点可能在你清单外，列 left_over）,
app/src/user_config/**（本地 workflow 扫描已删，检查剩余）、app/src/test_util/**（drive/workflows 残留）、app/src/tui_test_support.rs（LocalWorkflows singleton 删除）、app/src/uri/**（剩余 drive 残留）。`,
      },
    ]
    const results = await parallel(tasks.map((t) => () =>
      agent(t.prompt, { label: t.label, phase: "purge", schema: Report }),
    ))
    return results
  },
})
