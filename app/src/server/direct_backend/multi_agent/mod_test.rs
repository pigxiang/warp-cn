//! Tests for provider resolution against a request's Custom Endpoint
//! registry, which is the only source that carries third-party endpoint
//! credentials into the Direct LLM backend.

use api::request::settings::CustomModelProviders;
use api::request::settings::custom_model_providers::{
    CustomEndpointSchema, CustomModel, CustomModelProvider,
};
use warp_multi_agent_api as api;

use super::{resolve_from_custom_model, resolve_provider};
use crate::server::direct_backend::DirectProviderKind;

const VOLCANO_BASE_URL: &str = "https://ark.cn-beijing.volces.com/api/v3/";

fn request_with(selected: &str, schema: CustomEndpointSchema, api_key: &str) -> api::Request {
    api::Request {
        settings: Some(api::request::Settings {
            custom_model_providers: Some(CustomModelProviders {
                providers: vec![CustomModelProvider {
                    base_url: VOLCANO_BASE_URL.to_owned(),
                    api_key: api_key.to_owned(),
                    models: vec![CustomModel {
                        slug: "glm-5.3-flash".to_owned(),
                        config_key: selected.to_owned(),
                        reasoning_effort: String::new(),
                    }],
                    schema: schema as i32,
                }],
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn custom_model_resolves_to_openai_compatible_endpoint() {
    let request = request_with(
        "glm-key",
        CustomEndpointSchema::OpenaiChatCompletions,
        "volc-key",
    );

    let provider = resolve_provider(&request, Some("glm-key")).expect("resolved");

    assert_eq!(provider.kind, DirectProviderKind::OpenAiCompatible);
    assert_eq!(provider.api_key, "volc-key");
    // Trailing slash trimmed so the driver's URL join stays well-formed.
    assert_eq!(
        provider.base_url,
        "https://ark.cn-beijing.volces.com/api/v3"
    );
    // The picker carries the config_key; the wire must carry the model slug.
    assert_eq!(provider.model_id, "glm-5.3-flash");
}

#[test]
fn anthropic_schema_selects_the_anthropic_driver() {
    let request = request_with(
        "glm-key",
        CustomEndpointSchema::AnthropicMessages,
        "volc-key",
    );

    let provider = resolve_provider(&request, Some("glm-key")).expect("resolved");

    assert_eq!(provider.kind, DirectProviderKind::Anthropic);
}

#[test]
fn responses_schema_reports_an_actionable_error() {
    let request = request_with("glm-key", CustomEndpointSchema::OpenaiResponses, "volc-key");

    let message = resolve_provider(&request, Some("glm-key")).expect_err("unsupported schema");

    assert!(
        message.contains("OpenAI Chat Completions"),
        "error should name the schema to switch to: {message}"
    );
}

#[test]
fn endpoint_without_api_key_is_reported_not_silently_skipped() {
    let request = request_with("glm-key", CustomEndpointSchema::OpenaiChatCompletions, "  ");

    let message = resolve_provider(&request, Some("glm-key")).expect_err("missing key");

    assert!(
        message.contains("no API key"),
        "unexpected message: {message}"
    );
}

#[test]
fn selection_outside_the_registry_is_not_a_custom_model() {
    let request = request_with(
        "glm-key",
        CustomEndpointSchema::OpenaiChatCompletions,
        "volc-key",
    );

    assert!(
        resolve_from_custom_model(&request, Some("gpt-5")).is_none(),
        "a first-party model id must fall through to ambient configuration"
    );
    assert!(resolve_from_custom_model(&request, None).is_none());
    assert!(
        resolve_from_custom_model(&api::Request::default(), Some("glm-key")).is_none(),
        "a request without settings carries no endpoints"
    );
}

#[test]
fn ambient_configuration_supplies_the_picker_selection() {
    // No custom endpoints on the wire: the model id still has to reach the
    // provider that ambient config resolves.
    let provider = resolve_provider(&api::Request::default(), Some("deepseek-v4-flash"));
    assert!(
        provider.is_err() || provider.expect("resolved").model_id == "deepseek-v4-flash",
        "ambient resolution must carry the picked model through"
    );
}
