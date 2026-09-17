//! Desktop session state for the narrow Tauri boundary.
//!
//! The webview never touches ADB or the recovery mirrors directly. It only
//! offers validated strings; this state binds every active operation to one
//! inspected device (serial + fingerprint), gates concurrent mutation, caches
//! the in-flight apply plan (resuming with the SAME plan keeps journal ids
//! aligned; re-planning mid-transaction would misalign them), and holds
//! bounded icon bytes. Cancellation is always a domain request (a
//! `Decision` handled by the transaction runner), never abrupt task
//! abandonment: this type has no thread handles to kill.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::{commands::CommandError, policy::Plan};

/// Maximum decoded bytes kept per catalog icon (mirrors the bridge bound).
pub const MAX_ICON_BYTES: usize = 1_048_576;
/// Maximum icons cached for one catalog; the bridge pages, so this is a cap,
/// not a target. Distinct from the per-icon byte bound: an overfull catalog
/// reports `icon-cache-full`, an oversize icon `icon-too-large`.
pub const MAX_ICONS: usize = 512;

#[derive(Debug, Default)]
struct Inner {
    serial: Option<String>,
    fingerprint: Option<String>,
    busy: bool,
    icons: HashMap<String, Vec<u8>>,
    plans: HashMap<String, Plan>,
}

/// Mutex-guarded session shared by all Tauri commands.
#[derive(Debug, Default)]
pub struct UnscrollState {
    inner: Mutex<Inner>,
}

/// Held for the duration of one mutating transaction. Dropping releases the
/// busy flag, so early returns and errors cannot wedge the session.
pub struct MutationGuard<'a> {
    state: &'a UnscrollState,
}

impl Drop for MutationGuard<'_> {
    fn drop(&mut self) {
        self.state.finish_mutation();
    }
}

impl UnscrollState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind the session to one inspected device. Rebinding (a new
    /// inspection) replaces the binding and drops stashed plans, which
    /// belong to the previous device context. Rejected while a mutation
    /// holds the session (`busy-transaction`): every rebinding command must
    /// first win the mutation guard, so a concurrent rebind is refused
    /// atomically instead of racing the running transaction.
    pub fn bind_device(&self, serial: &str, fingerprint: &str) -> Result<(), CommandError> {
        let mut inner = self.inner.lock().expect("session lock");
        if inner.busy {
            return Err(CommandError::busy());
        }
        inner.serial = Some(serial.to_owned());
        inner.fingerprint = Some(fingerprint.to_owned());
        inner.plans.clear();
        Ok(())
    }

    /// Bind after a command already holds the mutation guard (e.g. a
    /// successful inspection that validated while busy). Crate-visible only;
    /// external callers use the checked [`UnscrollState::bind_device`].
    pub(crate) fn bind_inspected(&self, serial: &str, fingerprint: &str) {
        let mut inner = self.inner.lock().expect("session lock");
        inner.serial = Some(serial.to_owned());
        inner.fingerprint = Some(fingerprint.to_owned());
        inner.plans.clear();
    }

    /// Reject device replacement during a session: the serial and
    /// fingerprint must equal the inspected binding.
    pub fn check_binding(&self, serial: &str, fingerprint: &str) -> Result<(), CommandError> {
        let inner = self.inner.lock().expect("session lock");
        let matches = inner.serial.as_deref() == Some(serial)
            && inner.fingerprint.as_deref() == Some(fingerprint);
        if matches {
            Ok(())
        } else {
            Err(CommandError::stale_device())
        }
    }

    /// Allow only one mutating transaction at a time. The second concurrent
    /// caller gets `busy-transaction` and must wait or offer a domain-level
    /// cancel (a `Decision`), never kill the running work.
    pub fn try_begin_mutation(&self) -> Result<(), CommandError> {
        let mut inner = self.inner.lock().expect("session lock");
        if inner.busy {
            return Err(CommandError::busy());
        }
        inner.busy = true;
        Ok(())
    }

    /// Begin a guarded mutation; the returned guard releases the session
    /// when dropped. Atomic under the same mutex as `try_begin_mutation`.
    pub fn begin_mutation(&self) -> Result<MutationGuard<'_>, CommandError> {
        self.try_begin_mutation()?;
        Ok(MutationGuard { state: self })
    }

    pub fn finish_mutation(&self) {
        self.inner.lock().expect("session lock").busy = false;
    }

    pub fn is_busy(&self) -> bool {
        self.inner.lock().expect("session lock").busy
    }

    /// Stash the apply plan for this serial so decisions and retries resume
    /// with the identical plan (identical journal ids). Discarded on rebind
    /// and on terminal outcomes; a missing entry across restarts is a
    /// fail-closed `plan-rejected`, never a re-plan over history.
    pub fn store_plan(&self, serial: &str, plan: Plan) {
        self.inner.lock().expect("session lock").plans.insert(serial.to_owned(), plan);
    }

    /// A clone of the stashed plan, if this session started one.
    pub fn plan_for(&self, serial: &str) -> Option<Plan> {
        self.inner.lock().expect("session lock").plans.get(serial).cloned()
    }

    pub fn discard_plan(&self, serial: &str) {
        self.inner.lock().expect("session lock").plans.remove(serial);
    }

    /// Cache one decoded icon. Rejects empty and oversize payloads
    /// (`icon-too-large`, matching what the `load_app_icon` read reports)
    /// and an overfull catalog (`icon-cache-full`) so a hostile bridge
    /// cannot exhaust desktop memory; handlers skip uncached icons.
    pub fn store_icon(&self, package: &str, bytes: Vec<u8>) -> Result<(), CommandError> {
        if bytes.is_empty() || bytes.len() > MAX_ICON_BYTES {
            return Err(CommandError::icon_too_large());
        }
        let mut inner = self.inner.lock().expect("session lock");
        if !inner.icons.contains_key(package) && inner.icons.len() >= MAX_ICONS {
            return Err(CommandError::icon_cache_full());
        }
        inner.icons.insert(package.to_owned(), bytes);
        Ok(())
    }

    pub fn take_icon(&self, package: &str) -> Option<Vec<u8>> {
        self.inner.lock().expect("session lock").icons.remove(package)
    }

    /// Non-removing read of one cached icon. Row re-renders must not lose
    /// icons, so the bounded `load_app_icon` command peeks here; only
    /// `take_icon` removes and only a new inspection clears the cache.
    pub fn peek_icon(&self, package: &str) -> Option<Vec<u8>> {
        self.inner.lock().expect("session lock").icons.get(package).cloned()
    }

    pub fn icon_count(&self) -> usize {
        self.inner.lock().expect("session lock").icons.len()
    }

    /// Release all cached icon bytes. Run on every new inspection because
    /// the catalog (and therefore every icon) may have changed. Dropping
    /// the `Vec<u8>` values frees the memory immediately.
    pub fn clear_icons(&self) {
        self.inner.lock().expect("session lock").icons.clear();
    }
}
