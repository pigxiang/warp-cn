//! `RemoteString`: marker type for strings that originate from the server.
//!
//! Wrapping a server-provided string in `RemoteString` enforces a type-level boundary:
//! the value cannot be passed to `warp_i18n::t!()` (which only accepts string literals)
//! and intentionally does not implement `From<RemoteString> for &str` or `Deref<Target=str>`.
//! Callers that need to render a `RemoteString` in the UI use `as_str()` *and* the CI
//! lint (`cargo xtask check-i18n`) ensures the result is not threaded into a translation
//! site.
//!
//! Conversions:
//! - `From<String>` is provided so server response codecs (cynic / serde) can construct
//!   values without ceremony.
//! - `Display` and `AsRef<str>` exist for direct UI rendering.
//! - There is **no** `From<RemoteString> for String` and **no** `&str` implicit deref:
//!   removing the wrapper requires an explicit `into_string()` call, which the lint can
//!   audit.

use std::fmt;

use crate::schema;

#[derive(cynic::Scalar, Debug, Clone, PartialEq, Eq, Hash)]
#[cynic(graphql_type = "String")]
pub struct RemoteString(String);

impl RemoteString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Explicit unwrap. Audited by `cargo xtask check-i18n`.
    pub fn into_string(self) -> String {
        self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for RemoteString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for RemoteString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for RemoteString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for RemoteString {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_renders_inner() {
        let r = RemoteString::new("hello from server");
        assert_eq!(format!("{r}"), "hello from server");
    }

    #[test]
    fn serde_roundtrip_is_transparent() {
        let r = RemoteString::new("payload");
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, "\"payload\"");
        let back: RemoteString = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }
}
