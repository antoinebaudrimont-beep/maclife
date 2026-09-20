//! Read-only X11 application identity inspection.

pub mod control;
pub mod identity;
pub mod input;
pub mod lifecycle;
pub mod model;
pub mod process;
pub mod report;
pub mod runtime;
pub mod x11;

use std::error::Error;

pub type DynError = Box<dyn Error + Send + Sync + 'static>;
