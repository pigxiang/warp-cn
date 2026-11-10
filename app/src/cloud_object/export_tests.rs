use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::channel::oneshot;
use parking_lot::Mutex;
use tempfile::TempDir;
use warp_util::path::ShellFamily;
use warpui::{AddSingletonModel, App, SingletonEntity, WindowId};

use super::{ExportEvent, ExportId, ExportManager, safe_filename};
use crate::cloud_object::CloudObjectTypeAndId;
use crate::cloud_object::model::persistence::CloudModel;
use crate::cloud_object::{
    CloudObjectMetadata, CloudObjectPermissions, ObjectIdType, ObjectType, Space,
};
use crate::cloud_object::{CloudWorkflow, CloudWorkflowModel, WorkflowId};
use crate::notebooks::{CloudNotebook, CloudNotebookModel, NotebookId};
use crate::server::ids::SyncId;
use crate::workspace::ToastStack;
use crate::workspaces::user_workspaces::UserWorkspaces;
use cloud_object_models::Workflow;

struct ExportTest {
    target_dir: TempDir,
    pending_exports: Arc<Mutex<HashMap<ExportId, oneshot::Sender<ExportEvent>>>>,
}

impl ExportTest {
    fn new(temp_dir_prefix: &str, app: &mut App) -> Self {
        let pending_exports = Arc::new(Mutex::new(
            HashMap::<ExportId, oneshot::Sender<ExportEvent>>::new(),
        ));

        {
            let pending_exports = pending_exports.clone();
            app.update(|ctx| {
                ctx.subscribe_to_model(&ExportManager::handle(ctx), move |_, event, _| {
                    let mut pending_exports = pending_exports.lock();
                    let id = match event {
                        ExportEvent::Canceled(id) => id,
                        ExportEvent::Failed { id, .. } => id,
                        ExportEvent::Completed { id, .. } => id,
                    };
                    if let Some(sender) = pending_exports.remove(id) {
                        let _ = sender.send(event.clone());
                    }
                });
            });
        }

        Self {
            target_dir: tempfile::Builder::new()
                .prefix(temp_dir_prefix)
                .tempdir()
                .expect("failed to create temporary export directory"),
            pending_exports,
        }
    }

    /// Starts exporting an object into the temporary directory.
    fn start_export(
        &self,
        export_ids: CloudObjectTypeAndId,
        app: &mut App,
    ) -> (ExportId, oneshot::Receiver<ExportEvent>) {
        let id = ExportId(export_ids, Space::Personal);
        let (tx, rx) = oneshot::channel();
        self.pending_exports.lock().insert(id, tx);

        ExportManager::handle(app).update(app, |export_manager, ctx| {
            let window_id = WindowId::new();
            export_manager.export(window_id, &[export_ids], ctx);
            export_manager.handle_files_picked(
                vec![id],
                Ok(vec![
                    self.target_dir
                        .path()
                        .to_str()
                        .expect("Path must be UTF-8")
                        .to_owned(),
                ]),
                ShellFamily::Posix,
                ctx,
            );
            id
        });

        (id, rx)
    }

    /// Get an export path, given the expected name.
    fn path(&self, name: impl AsRef<Path>, space: Option<Space>, app: &App) -> PathBuf {
        if let Some(space) = space {
            let space_name = app.read(|ctx| space.name(ctx));
            self.target_dir.path().join(space_name).join(name)
        } else {
            self.target_dir.path().join(name)
        }
    }
}

fn initialize_app(app: &mut App) {
    app.add_singleton_model(CloudModel::mock);
    app.add_singleton_model(ExportManager::new);
    app.add_singleton_model(UserWorkspaces::default_mock);
    app.add_singleton_model(|_| ToastStack);
}

/// Add a mocked workflow.
fn add_workflow(id: SyncId, workflow: Workflow, app: &mut App) {
    CloudModel::handle(app).update(app, |cloud_model, _ctx| {
        cloud_model.add_object(
            id,
            CloudWorkflow::new(
                id,
                CloudWorkflowModel::new(workflow),
                CloudObjectMetadata::mock(),
                CloudObjectPermissions::mock_personal(),
            ),
        );
    });
}

/// Add a mocked notebook.
fn add_notebook(id: SyncId, title: impl Into<String>, data: impl Into<String>, app: &mut App) {
    CloudModel::handle(app).update(app, |cloud_model, _ctx| {
        cloud_model.add_object(
            id,
            CloudNotebook::new(
                id,
                CloudNotebookModel {
                    title: title.into(),
                    data: data.into(),
                    ai_document_id: None,
                    conversation_id: None,
                },
                CloudObjectMetadata::mock(),
                CloudObjectPermissions::mock_personal(),
            ),
        );
    });
}

#[test]
fn test_export_notebook_duplicate() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);

        let notebook_id = SyncId::ServerId(NotebookId::from(456).into());
        add_notebook(
            notebook_id,
            "Test notebook",
            "This is a test notebook",
            &mut app,
        );

        let exporter = ExportTest::new("test_export_notebook_duplicate", &mut app);

        // Create a file at the default export path.
        fs::write(
            exporter.path("Test notebook.md", None, &app),
            "Already exists",
        )
        .expect("failed to write existing notebook");

        let (id, export) = exporter.start_export(
            CloudObjectTypeAndId::from_id_and_type(notebook_id, ObjectType::Notebook),
            &mut app,
        );
        let expected_path = exporter.path("Test notebook (1).md", None, &app);

        // The export should succeed, and not overwrite the existing file.
        assert_eq!(
            export.await,
            Ok(ExportEvent::Completed {
                id,
                path: expected_path.clone()
            })
        );

        assert_eq!(
            fs::read_to_string(exporter.path("Test notebook.md", None, &app))
                .expect("failed to read original file"),
            "Already exists"
        );

        let contents =
            fs::read_to_string(&expected_path).expect("failed to read exported notebook");
        assert_eq!(&contents, "This is a test notebook");
    });
}

#[test]
fn test_export_notebook_failure() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);

        let notebook_id = SyncId::ServerId(NotebookId::from(456).into());
        add_notebook(
            notebook_id,
            "Test notebook",
            "This is a test notebook",
            &mut app,
        );

        let exporter = ExportTest::new("test_export_notebook_failure", &mut app);
        // Ensure that the export will fail.
        fs::remove_dir_all(exporter.target_dir.path()).expect("Could not remove test directory");

        let (id, export) = exporter.start_export(
            CloudObjectTypeAndId::from_id_and_type(notebook_id, ObjectType::Notebook),
            &mut app,
        );

        // The export should error.
        assert_eq!(export.await, Ok(ExportEvent::Failed { id }));
    });
}

#[test]
fn test_export_notebook_with_embeds() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);

        let workflow_id = SyncId::ServerId(WorkflowId::from(123).into());
        let workflow = Workflow::new("Test workflow", "echo hello world");
        add_workflow(workflow_id, workflow, &mut app);

        let notebook_id = SyncId::ServerId(NotebookId::from(456).into());
        add_notebook(
            notebook_id,
            "Test notebook",
            format!(
                r#"
# This is a notebook
It has *text*.
```warp-embedded-object
id: {}
```

This is code:
```Python
print("hello")
```
"#,
                workflow_id.sqlite_uid_hash(ObjectIdType::Workflow)
            ),
            &mut app,
        );

        let exporter = ExportTest::new("test_export_notebook_with_embeds", &mut app);
        let (id, export) = exporter.start_export(
            CloudObjectTypeAndId::from_id_and_type(notebook_id, ObjectType::Notebook),
            &mut app,
        );
        let expected_path = exporter.path("Test notebook.md", None, &app);

        // The export should succeed.
        assert_eq!(
            export.await,
            Ok(ExportEvent::Completed {
                id,
                path: expected_path.clone()
            })
        );

        let contents =
            fs::read_to_string(&expected_path).expect("failed to read exported notebook");
        assert_eq!(
            contents,
            r#"
# This is a notebook
It has *text*\.
```warp-embedded-object
---
name: Test workflow
command: echo hello world
tags: []
description: ~
arguments: []
source_url: ~
author: ~
author_url: ~
shells: []
environment_variables: ~
id: Workflow-test_uid00000000000123

```

This is code:
```python
print("hello")
```
"#
        );
    });
}

#[test]
fn test_export_untitled_notebook() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let notebook_id = SyncId::ServerId(NotebookId::from(456).into());
        add_notebook(notebook_id, "", "This is untitled", &mut app);

        let exporter = ExportTest::new("test_export_untitled_notebook", &mut app);
        let (id, export) = exporter.start_export(
            CloudObjectTypeAndId::from_id_and_type(notebook_id, ObjectType::Notebook),
            &mut app,
        );
        let expected_path = exporter.path("Untitled.md", None, &app);

        // The export should succeed.
        assert_eq!(
            export.await,
            Ok(ExportEvent::Completed {
                id,
                path: expected_path.clone()
            })
        );

        let contents =
            fs::read_to_string(&expected_path).expect("failed to read exported notebook");
        assert_eq!(&contents, "This is untitled");
    });
}

#[test]
fn test_export_with_special_characters() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);

        let notebook_id = SyncId::ServerId(NotebookId::from(456).into());
        add_notebook(
            notebook_id,
            "Prefix: Some/notebook",
            "This is a test notebook",
            &mut app,
        );

        let exporter = ExportTest::new("test_export_with_special_characters", &mut app);
        let (id, export) = exporter.start_export(
            CloudObjectTypeAndId::from_id_and_type(notebook_id, ObjectType::Notebook),
            &mut app,
        );
        let expected_path = exporter.path("Prefix_ Some_notebook.md", None, &app);

        // The export should succeed, and transform the path.
        assert_eq!(
            export.await,
            Ok(ExportEvent::Completed {
                id,
                path: expected_path.clone()
            })
        );
    });
}

#[test]
fn test_safe_filename() {
    for (expected_in, expected_out) in [
        (
            "allowed $special %characters",
            "allowed $special %characters",
        ),
        ("warp:drive", "warp_drive"),
        ("a/b/c/d:e", "a_b_c_d_e"),
        ("the\0sneaky\0null", "the_sneaky_null"),
        ("ascii\x03control\x1bchars", "ascii_control_chars"),
    ] {
        assert_eq!(safe_filename(expected_in), expected_out);
    }
}

#[test]
fn test_export_multiple_objects() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);

        // Create three notebooks (workflows were removed in the lite fork; the
        // bulk-export path is exercised with the remaining exportable type).
        let notebook_id1 = SyncId::ServerId(NotebookId::from(789).into());
        add_notebook(
            notebook_id1,
            "Test notebook 1",
            "This is notebook one",
            &mut app,
        );

        let notebook_id2 = SyncId::ServerId(NotebookId::from(790).into());
        add_notebook(
            notebook_id2,
            "Test notebook 2",
            "This is notebook two",
            &mut app,
        );

        let notebook_id3 = SyncId::ServerId(NotebookId::from(791).into());
        add_notebook(
            notebook_id3,
            "Test notebook 3",
            "This is notebook three",
            &mut app,
        );

        let exporter = ExportTest::new("test_export_multiple_objects", &mut app);

        // Prepare export IDs for all three objects
        let export_ids = vec![
            CloudObjectTypeAndId::from_id_and_type(notebook_id1, ObjectType::Notebook),
            CloudObjectTypeAndId::from_id_and_type(notebook_id2, ObjectType::Notebook),
            CloudObjectTypeAndId::from_id_and_type(notebook_id3, ObjectType::Notebook),
        ];

        // Create channels for all exports
        let mut receivers = Vec::new();
        {
            let mut pending_exports = exporter.pending_exports.lock();
            for &id in &export_ids {
                let (tx, rx) = oneshot::channel();
                pending_exports.insert(ExportId(id, Space::Personal), tx);
                receivers.push(rx);
            }
        }

        ExportManager::handle(&app).update(&mut app, |export_manager, ctx| {
            let window_id = WindowId::new();
            export_manager.export(window_id, &export_ids, ctx);

            let all_export_ids = export_ids
                .iter()
                .map(|&id| ExportId(id, Space::Personal))
                .collect::<Vec<_>>();
            export_manager.handle_files_picked(
                all_export_ids,
                Ok(vec![
                    exporter
                        .target_dir
                        .path()
                        .to_str()
                        .expect("Path must be UTF-8")
                        .to_owned(),
                ]),
                ShellFamily::Posix,
                ctx,
            );
        });

        // Wait for all exports to complete
        for rx in receivers {
            let result = rx.await;
            assert!(
                matches!(result, Ok(ExportEvent::Completed { .. })),
                "Export failed or was canceled"
            );
        }

        // Verify the contents of each exported file
        let notebook1_path = exporter.path("Test notebook 1.md", Some(Space::Personal), &app);
        let notebook1_contents =
            fs::read_to_string(&notebook1_path).expect("Failed to read notebook 1");
        assert!(notebook1_contents.contains("This is notebook one"));

        let notebook2_path = exporter.path("Test notebook 2.md", Some(Space::Personal), &app);
        let notebook2_contents =
            fs::read_to_string(&notebook2_path).expect("Failed to read notebook 2");
        assert!(notebook2_contents.contains("This is notebook two"));

        let notebook3_path = exporter.path("Test notebook 3.md", Some(Space::Personal), &app);
        let notebook3_contents =
            fs::read_to_string(&notebook3_path).expect("Failed to read notebook 3");
        assert!(notebook3_contents.contains("This is notebook three"));

        // Check that all files were created
        assert!(notebook1_path.exists(), "Notebook 1 file does not exist");
        assert!(notebook2_path.exists(), "Notebook 2 file does not exist");
        assert!(notebook3_path.exists(), "Notebook 3 file does not exist");
    });
}
