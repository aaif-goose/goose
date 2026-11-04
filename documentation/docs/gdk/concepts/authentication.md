---
title: Authentication
description: Supply credentials to model providers without coupling the agent runtime to a login flow or secret store.
displayed_sidebar: gdkSidebar
---

# Authentication

A provider needs a way to authenticate requests to its backend. Where the
credential comes from is an application decision: an environment variable, a
secret store, an existing user login, or an organization-specific credential
service. That choice should not require changing the agent's operations or
conversation format.

GDK separates the HTTP authentication mechanism from the provider's request
format. `ApiClient` accepts an `AuthMethod`; a custom method can ask your
application for the current credential whenever it prepares a request. Provider
implementations can also have their own authentication flows where a service
requires them.

## Authentication mechanisms

The following mechanisms are defined in `goose-providers::api_client`:

| Mechanism                 | Use                                                                                  |
| ------------------------- | ------------------------------------------------------------------------------------ |
| `AuthMethod::NoAuth`      | An endpoint that needs no authentication, such as a suitably isolated local service. |
| `AuthMethod::BearerToken` | A token sent in the `Authorization: Bearer ...` header.                              |
| `AuthMethod::ApiKey`      | A credential sent under a specified header name.                                     |
| `AuthMethod::Custom`      | An `AuthProvider` that supplies a header name and value at request time.             |

For example, an application can pass a key directly rather than rely on the
goose CLI configuration:

```rust
let client = ApiClient::new_with_tls(
    "https://api.openai.com".into(),
    AuthMethod::BearerToken(std::env::var("OPENAI_API_KEY")?),
    None,
)?;
```

This configures how the credential is sent; it does not install a secret store
or start a login flow. The TLS argument is separate from HTTP authentication.
Use a TLS-enabled build for HTTPS, and supply `TlsConfig` if your environment
requires custom certificate authorities or a client certificate.

## Provider-specific and declarative authentication

The Databricks implementation supports token and OAuth configurations through
`DatabricksAuth`. Its auth provider accepts application-supplied callbacks for
obtaining tokens; GDK does not assume every host has the same browser login or
credential storage.

[Declarative providers](/docs/gdk/concepts/providers#declarative-providers) can
name an API-key variable and resolve it through `KeyResolver`. The supplied
`EnvKeyResolver` uses the process environment. A definition can alternatively
configure a command that obtains a credential, with caching and refresh
settings. Those commands are executable configuration, so only load definitions
you trust.

## Supplying your own authentication

Use `AuthProvider` when the credential can change during the application's
lifetime or comes from a source the built-in mechanisms do not cover. For
example, the host can keep the token from its own login flow in shared state:

```rust
use std::sync::{Arc, RwLock};

use anyhow::Result;
use async_trait::async_trait;
use goose_providers::api_client::AuthProvider;

struct AppAuth {
    token: Arc<RwLock<String>>,
}

#[async_trait]
impl AuthProvider for AppAuth {
    async fn get_auth_header(&self) -> Result<(String, String)> {
        let token = self.token.read().unwrap();
        Ok(("Authorization".into(), format!("Bearer {token}")))
    }
}
```

Pass it to `ApiClient` as `AuthMethod::Custom(Box::new(AppAuth { token }))`.
The host can update the shared token after login or rotation without rebuilding
the agent. A real credential store can replace the shared string while keeping
the same trait boundary.

`AuthProvider` also has a `refresh_credentials` method. Its default reports that
refresh is unsupported; implement it when your flow can refresh credentials.
The provider or application decides when to invoke it. A custom header callback
alone does not implement OAuth or automatic refresh.

## Keep the boundaries separate

Credentials should not be part of the conversation, prompts, tool output, or
saved effects. Keep them in the application's credential handling, and avoid
logging their values. A working directory is not a boundary around what an
agent or its tools can access.

Authenticating a model request is also different from authenticating a client
connection to goose. If your application uses the pre-built binary over ACP,
the backend's provider credentials belong on the backend; a `goose serve`
connection has its own authentication. See
[Build an app with goose](/docs/gdk/acp#use-a-server-instead) for that path.

The full HTTP authentication and TLS APIs are in
[`goose-providers::api_client`](https://docs.rs/goose-providers/latest/goose_providers/api_client/).
