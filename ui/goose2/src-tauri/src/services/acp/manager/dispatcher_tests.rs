use super::*;

#[test]
fn extract_user_message_strips_xml_wrapper() {
    let wrapped = "<persona-instructions>\nYou are a helpful assistant.\n</persona-instructions>\n\n<user-message>\nhello\n</user-message>";
    assert_eq!(extract_user_message(wrapped), "hello");
}

#[test]
fn extract_user_message_multiline() {
    let wrapped = "<persona-instructions>\nstuff\n</persona-instructions>\n\n<user-message>\nline one\nline two\n</user-message>";
    assert_eq!(extract_user_message(wrapped), "line one\nline two");
}

#[test]
fn extract_user_message_no_wrapper() {
    assert_eq!(extract_user_message("plain text"), "plain text");
}

#[test]
fn extract_user_message_preserves_inner_delimiter() {
    // User literally typed "</user-message>" in their message — must not truncate.
    let wrapped = "<persona-instructions>\nstuff\n</persona-instructions>\n\n<user-message>\ncheck this tag: </user-message> cool right?\n</user-message>";
    assert_eq!(
        extract_user_message(wrapped),
        "check this tag: </user-message> cool right?"
    );
}
