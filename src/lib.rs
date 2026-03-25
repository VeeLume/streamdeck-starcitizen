//! Shared library crate for streamdeck-starcitizen binaries.
//!
//! The plugin binary (`src/main.rs`) and tool binaries (`src/bin/`) share
//! these modules.  Only modules needed by external binaries are `pub`.

pub mod bindings;
pub mod discovery;
