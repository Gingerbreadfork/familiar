//! Logitech HID++ 2.0 protocol.
//!
//! [`Link`] owns a [`Transport`], matches replies to requests and passes events
//! on. [`Device`] addresses one device on a link, resolves feature IDs to
//! indices and checks every call against [`policy`]. The [`features`] modules give typed
//! access on top of any [`Requester`].

mod device;
mod error;
pub mod features;
mod link;
pub mod policy;
pub mod receiver;
mod report;
mod transport;

#[cfg(any(test, feature = "mock"))]
pub mod mock;
#[cfg(any(test, feature = "mock"))]
pub mod sim;

pub use device::{DIRECT, Device, FeatureEntry, FeatureSlot, Requester};
pub use error::{Error, FeatureError, RegisterError};
pub use link::{DAEMON_SWIDS, Exchange, Incoming, Link, TOOL_SWIDS, is_event};
pub use policy::Policy;
pub use report::{Packet, ReportKind};
pub use transport::{Transport, Wake, Writer};
