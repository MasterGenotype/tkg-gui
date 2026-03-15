use std::sync::{Arc, OnceLock};

/// Returns a shared ureq Agent configured with a native-tls backend.
///
/// The default `ureq::get()` convenience functions use an agent built with
/// `AgentBuilder::new()`, which has NO TLS connector when the `tls` (rustls)
/// feature is disabled. The `native-tls` feature only makes the API available
/// but does not auto-configure the default agent. This function provides a
/// properly configured agent for all HTTPS requests.
pub fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        let connector = native_tls::TlsConnector::new()
            .expect("Failed to initialise native-tls TLS connector");
        ureq::AgentBuilder::new()
            .tls_connector(Arc::new(connector))
            .build()
    })
}
