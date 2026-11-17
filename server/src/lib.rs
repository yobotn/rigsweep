//! rigsweep-stats: receives RigSweep's anonymous weekly totals, serves the public counters on
//! rigsweep.com and the owner's admin dashboard. Source is public so anyone can check what is kept.

pub mod model;
pub mod store;
pub mod web;

pub use store::MIGRATOR;
pub use web::{router, Config, Geo};
