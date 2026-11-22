//! Browser policy building blocks for the Herdr WebView fork.
//!
//! This portable crate deliberately owns no app window, persistence store,
//! native approval authority or process-global browser session. The app host
//! owns those concerns and supplies backend availability explicitly.
#![forbid(unsafe_code)]

pub mod engine_policy;
pub mod url_policy;
