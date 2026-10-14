//! Safe wrappers around the NDI receive API.

mod ffi;
mod finder;
mod query;
mod receiver;
mod runtime;
mod scope;

pub use finder::SourceFinder;
pub use query::SourceQuery;
pub use receiver::{Capture, Receiver, VideoFrame};
pub use runtime::NdiRuntime;
pub use scope::DiscoveryScope;
