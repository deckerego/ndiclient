//! Safe wrappers around the NDI receive API.

mod ffi;
mod finder;
mod frame_sync;
mod query;
mod receiver;
mod runtime;
mod scope;

pub use finder::SourceFinder;
pub use frame_sync::{FrameStamp, FrameSync, VideoFrame};
pub use query::SourceQuery;
pub use receiver::Receiver;
pub use runtime::NdiRuntime;
pub use scope::DiscoveryScope;
