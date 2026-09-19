//! Read-only X11 application identity inspection.

pub mod identity;
pub mod model;
pub mod process;
pub mod report;
pub mod x11;

use std::error::Error;

pub type DynError = Box<dyn Error + Send + Sync + 'static>;
