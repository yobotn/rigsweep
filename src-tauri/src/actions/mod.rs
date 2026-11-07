pub mod defender;
pub mod power;
pub mod registry;
pub mod service;
pub mod task;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::cell::RefCell;

use crate::catalog::{validate_action, Action};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "state", content = "detail", rename_all = "snake_case")]
pub enum Status {
    Applied,
    NotApplied,
    /// Target doesn't exist on this Windows edition (counts as nothing to do).
    NotPresent,
    Error(String),
}

/// Per-operation context. Caches expensive lookups (Defender prefs) and is
/// invalidated after every change.
pub struct Ctx {
    pub dev_folders: Vec<String>,
    pub power_dup_guid: Option<String>,
    mp: RefCell<Option<defender::MpPrefs>>,
}

impl Ctx {
    pub fn new(dev_folders: Vec<String>, power_dup_guid: Option<String>) -> Ctx {
        Ctx { dev_folders, power_dup_guid, mp: RefCell::new(None) }
    }

    pub fn mp_prefs(&self) -> Result<defender::MpPrefs> {
        if let Some(p) = self.mp.borrow().as_ref() {
            return Ok(p.clone());
        }
        let p = defender::read_prefs()?;
        *self.mp.borrow_mut() = Some(p.clone());
        Ok(p)
    }

    pub fn invalidate(&self) {
        *self.mp.borrow_mut() = None;
    }
}

pub fn status(a: &Action, ctx: &Ctx) -> Status {
    let r = match a {
        Action::Service { name, start, .. } => service::status(name, *start),
        Action::Task { path } => task::status(path),
        Action::Registry { hive, path, value, kind, data } => registry::status(*hive, path, value, *kind, data),
        Action::DefenderExclusion { processes, use_dev_folders } => {
            defender::status(ctx, processes, *use_dev_folders)
        }
        Action::PowerPlan { scheme } => power::status(scheme, ctx),
    };
    r.unwrap_or_else(|e| Status::Error(e.to_string()))
}

/// Capture the current (original) value so it can be restored later.
pub fn read_current(a: &Action, ctx: &Ctx) -> Result<Value> {
    match a {
        Action::Service { name, .. } => service::read_current(name),
        Action::Task { path } => task::read_current(path),
        Action::Registry { hive, path, value, .. } => registry::read_current(*hive, path, value),
        Action::DefenderExclusion { processes, use_dev_folders } => {
            defender::read_current(ctx, processes, *use_dev_folders)
        }
        Action::PowerPlan { .. } => power::read_current(),
    }
}

pub fn apply(a: &Action, ctx: &mut Ctx) -> Result<Option<String>> {
    validate_action(a).map_err(anyhow::Error::msg)?;
    let r = match a {
        Action::Service { name, start, stop } => service::apply(name, *start, *stop).map(|_| None),
        Action::Task { path } => task::set_enabled(path, false).map(|_| None),
        Action::Registry { hive, path, value, kind, data } => {
            registry::apply(*hive, path, value, *kind, data).map(|_| None)
        }
        Action::DefenderExclusion { processes, use_dev_folders } => {
            defender::apply(ctx, processes, *use_dev_folders).map(|_| None)
        }
        Action::PowerPlan { scheme } => power::apply(scheme, ctx),
    };
    ctx.invalidate();
    r
}

pub fn revert(a: &Action, original: &Value, ctx: &Ctx) -> Result<()> {
    let r = match a {
        Action::Service { name, .. } => service::revert(name, original),
        Action::Task { path } => task::revert(path, original),
        Action::Registry { hive, path, value, .. } => registry::revert(*hive, path, value, original),
        Action::DefenderExclusion { processes, use_dev_folders } => {
            defender::revert(ctx, processes, *use_dev_folders, original)
        }
        Action::PowerPlan { .. } => power::revert(original),
    };
    ctx.invalidate();
    r
}
