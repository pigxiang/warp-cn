use warpui::async_assert;
use warpui::integration::AssertionCallback;

use crate::cloud_object::model::generic_string_model::GenericStringObjectId;
use crate::cloud_object::model::persistence::CloudModel;
use crate::cloud_object::{CloudModelType, GenericCloudObject, Revision};
use crate::notebooks::{CloudNotebookModel, NotebookId};
use crate::server::ids::{HashableId, ServerId, SyncId, ToServerId};
use crate::settings::{CloudPreferenceModel, Preference};

/// Asserts metadata exists for the object with the given key and that the revision in that
/// metadata matches the given expected revision.
pub fn assert_metadata_revision<K, M>(id: &str, expected_revision: i64) -> AssertionCallback
where
    K: HashableId + ToServerId + std::fmt::Debug + Into<String> + Clone + 'static,
    M: CloudModelType<IdType = K, CloudObjectType = GenericCloudObject<K, M>> + 'static,
{
    let id = SyncId::ServerId(ServerId::try_from(id).expect("ID is invalid"));
    Box::new(move |app, _window_id| {
        let revision =
            app.get_singleton_model_handle::<CloudModel>()
                .read(app, |cloud_model, _| {
                    let object = cloud_model
                        .get_object_of_type::<K, M>(&id)
                        .expect("object should exist");
                    object.metadata.revision.expect("revision should exist")
                });
        async_assert!(
            revision
                == Revision::from_unix_timestamp_micros(expected_revision)
                    .expect("revision should parse"),
            "Expected revision to be:{expected_revision:?}\nBut got:\n{revision:?}"
        )
    })
}

/// Asserts that a json preference object with the given contents exists in the SQLite db.
pub fn assert_cloud_preference_exists(expected_preference: Preference) -> AssertionCallback {
    Box::new(move |app, _window_id| {
        let stored_preference =
            app.get_singleton_model_handle::<CloudModel>()
                .read(app, |cloud_model, _| {
                    let object = cloud_model
                        .get_all_objects_of_type::<GenericStringObjectId, CloudPreferenceModel>()
                        .find(|p| p.model().string_model == expected_preference)
                        .expect("Expected to find a matching preference object");
                    object.model().string_model.clone()
                });
        async_assert!(
            expected_preference == stored_preference,
            "Expected json object contents to match:\n{expected_preference:?}\nBut got:\n{stored_preference:?}"
        )
    })
}

/// Asserts metadata exists for the notebook object with the given key and that the revision in
/// that metadata matches the given expected revision.
pub fn assert_notebook_metadata_revision(
    id: impl AsRef<str>,
    expected_revision: i64,
) -> AssertionCallback {
    assert_metadata_revision::<NotebookId, CloudNotebookModel>(id.as_ref(), expected_revision)
}
