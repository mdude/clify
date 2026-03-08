//! clify-core: Spec parsing, validation, and code generation for Clify.

pub mod spec;
pub mod validator;
pub mod generator;
pub mod schema;
pub mod scanner;
pub mod skills;

pub use spec::ClifySpec;
