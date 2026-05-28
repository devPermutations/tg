//! Allowlist mutations (`tg allow`, `tg deny`, `tg list`,
//! `tg set-owner`) and pairing-flow handlers (`tg pair`,
//! `tg pending`, `tg reject`).
//!
//! Split into `allowlist` (configuration mutations + AllowError) and
//! `pairing` (pending-pairings CLI handlers). External callers should
//! treat `crate::access::*` as the public surface — both submodules'
//! contents are re-exported here.

mod allowlist;
mod pairing;

pub use allowlist::{
    allow, append_allow, client_from_config, deny, list, set_owner,
    AllowError,
};
pub use pairing::{pair, pending, reject};
