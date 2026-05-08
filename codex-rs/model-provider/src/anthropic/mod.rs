mod catalog;

use std::path::PathBuf;
use std::sync::Arc;

use codex_api::AuthProvider;
use codex_api::Provider;
use codex_api::SharedAuthProvider;
use codex_login::AuthManager;
use codex_login::CodexAuth;
use codex_model_provider_info::ModelProviderInfo;
use codex_models_manager::manager::SharedModelsManager;
use codex_models_manager::manager::StaticModelsManager;
use codex_protocol::account::ProviderAccount;
use codex_protocol::error::Result;
use codex_protocol::openai_models::ModelsResponse;
use http::HeaderMap;
use http::HeaderValue;

use crate::provider::ModelProvider;
use crate::provider::ProviderAccountResult;
use crate::provider::ProviderAccountState;
use crate::provider::ProviderCapabilities;
pub(crate) use catalog::static_model_catalog;

#[derive(Clone, Debug)]
pub(crate) struct AnthropicModelProvider {
    info: ModelProviderInfo,
}

impl AnthropicModelProvider {
    pub(crate) fn new(provider_info: ModelProviderInfo) -> Self {
        Self {
            info: provider_info,
        }
    }
}

#[async_trait::async_trait]
impl ModelProvider for AnthropicModelProvider {
    fn info(&self) -> &ModelProviderInfo {
        &self.info
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            namespace_tools: true,
            image_generation: false,
            web_search: false,
        }
    }

    fn auth_manager(&self) -> Option<Arc<AuthManager>> {
        None
    }

    async fn auth(&self) -> Option<CodexAuth> {
        None
    }

    fn account_state(&self) -> ProviderAccountResult {
        Ok(ProviderAccountState {
            account: Some(ProviderAccount::ApiKey),
            requires_openai_auth: false,
        })
    }

    async fn api_provider(&self) -> Result<Provider> {
        self.info.to_api_provider(/*auth_mode*/ None)
    }

    async fn api_auth(&self) -> Result<SharedAuthProvider> {
        let api_key = self.info.api_key()?;
        Ok(Arc::new(AnthropicAuthProvider { api_key }))
    }

    fn models_manager(
        &self,
        _codex_home: PathBuf,
        config_model_catalog: Option<ModelsResponse>,
    ) -> SharedModelsManager {
        Arc::new(StaticModelsManager::new(
            /*auth_manager*/ None,
            config_model_catalog.unwrap_or_else(static_model_catalog),
        ))
    }
}

#[derive(Clone, Debug)]
struct AnthropicAuthProvider {
    api_key: Option<String>,
}

impl AuthProvider for AnthropicAuthProvider {
    fn add_auth_headers(&self, headers: &mut HeaderMap) {
        if let Some(api_key) = self.api_key.as_ref()
            && let Ok(header) = HeaderValue::from_str(api_key)
        {
            let _ = headers.insert("x-api-key", header);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_model_provider_info::ModelProviderInfo;
    use pretty_assertions::assert_eq;

    #[test]
    fn anthropic_capabilities_disable_openai_hosted_tools() {
        let provider = AnthropicModelProvider::new(ModelProviderInfo::create_anthropic_provider(
            /*base_url*/ None,
        ));

        assert_eq!(
            provider.capabilities(),
            ProviderCapabilities {
                namespace_tools: true,
                image_generation: false,
                web_search: false,
            }
        );
    }

    #[test]
    fn anthropic_auth_uses_x_api_key_header() {
        let auth = AnthropicAuthProvider {
            api_key: Some("test-key".to_string()),
        };
        let mut headers = HeaderMap::new();

        auth.add_auth_headers(&mut headers);

        assert_eq!(
            headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("test-key")
        );
        assert!(!headers.contains_key(http::header::AUTHORIZATION));
    }
}
