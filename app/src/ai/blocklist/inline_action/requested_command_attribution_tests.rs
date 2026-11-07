use super::*;

#[test]
fn test_is_command_copied_from_notebook() {
    let notebook = CloudNotebookModel {
        title: String::from(""),
        data: String::from("hello world\n```\n foobar \n```\n"),
        ai_document_id: None,
        conversation_id: None,
    };
    assert!(!is_command_copied_from_notebook("hello world", &notebook));
    assert!(!is_command_copied_from_notebook("foo", &notebook));
    assert!(is_command_copied_from_notebook("foobar", &notebook));
}
