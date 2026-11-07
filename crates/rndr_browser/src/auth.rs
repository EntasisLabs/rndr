//! How an authentication challenge is answered.
//!
//! Server trust is the system's certificate check. The decision for that
//! method is default handling, and no credential is supplied, so an invalid
//! certificate stays rejected. Every other method is a credential flow this
//! slice does not support: it is cancelled, and no panel is opened.

#![cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]

/// What the platform handler tells WebKit to do with one challenge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuthDecision {
    /// System handling. For server trust this is normal certificate validation.
    PerformDefaultHandling,
    /// Refuse the challenge. No credential is created or sent.
    Cancel,
}

/// `method` is the protection space's authentication method name.
///
/// `NSURLAuthenticationMethodServerTrust` is the only method that uses the
/// system validator. HTTP basic, digest, form, NTLM, negotiate, client
/// certificates, and unknown methods are cancelled. This function never
/// selects a path that installs a trust credential.
pub(crate) fn decide_authentication(method: &str) -> AuthDecision {
    if method == "NSURLAuthenticationMethodServerTrust" {
        AuthDecision::PerformDefaultHandling
    } else {
        AuthDecision::Cancel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_trust_uses_system_validation_and_credentials_are_refused() {
        assert_eq!(
            decide_authentication("NSURLAuthenticationMethodServerTrust"),
            AuthDecision::PerformDefaultHandling
        );
        for method in [
            "NSURLAuthenticationMethodDefault",
            "NSURLAuthenticationMethodHTTPBasic",
            "NSURLAuthenticationMethodHTTPDigest",
            "NSURLAuthenticationMethodHTMLForm",
            "NSURLAuthenticationMethodNTLM",
            "NSURLAuthenticationMethodNegotiate",
            "NSURLAuthenticationMethodClientCertificate",
            "",
            "custom-scheme",
        ] {
            assert_eq!(
                decide_authentication(method),
                AuthDecision::Cancel,
                "{method}"
            );
        }
    }
}
