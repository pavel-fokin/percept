//! Presentation layer: the command-line interface.

mod address;
mod command;
mod hook;
mod maps;
mod projects;
mod status;

pub use command::run;
