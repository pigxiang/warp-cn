use super::*;

#[test]
fn adds_version_to_bare_host() {
    assert_eq!(
        api_url("https://api.openai.com", "v1", "chat/completions"),
        "https://api.openai.com/v1/chat/completions"
    );
    assert_eq!(
        api_url("https://api.openai.com/", "v1", "chat/completions"),
        "https://api.openai.com/v1/chat/completions"
    );
}

#[test]
fn keeps_existing_version_segment() {
    assert_eq!(
        api_url("https://api.openai.com/v1", "v1", "chat/completions"),
        "https://api.openai.com/v1/chat/completions"
    );
    assert_eq!(
        api_url(
            "https://ark.cn-beijing.volces.com/api/v3",
            "v1",
            "chat/completions"
        ),
        "https://ark.cn-beijing.volces.com/api/v3/chat/completions"
    );
    assert_eq!(
        api_url("https://proxy.example.com/v1beta", "v1beta", "models"),
        "https://proxy.example.com/v1beta/models"
    );
}

#[test]
fn keeps_full_endpoint_url() {
    assert_eq!(
        api_url(
            "https://ark.cn-beijing.volces.com/api/v3/chat/completions",
            "v1",
            "chat/completions"
        ),
        "https://ark.cn-beijing.volces.com/api/v3/chat/completions"
    );
}

#[test]
fn hosts_are_not_mistaken_for_versions() {
    assert_eq!(
        api_url("https://api.deepseek.com", "v1", "models"),
        "https://api.deepseek.com/v1/models"
    );
    assert_eq!(
        api_url("https://v2.example.com", "v1", "models"),
        "https://v2.example.com/v1/models"
    );
}

#[test]
fn gemini_version_is_used_for_the_default_host() {
    assert_eq!(
        api_url(
            "https://generativelanguage.googleapis.com",
            "v1beta",
            "models/gemini-2.5-flash:streamGenerateContent"
        ),
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:streamGenerateContent"
    );
}
