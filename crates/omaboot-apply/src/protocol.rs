//! The number the engine and the helper agree on.
//!
//! This one file is compiled into both binaries (`omaboot` includes it by
//! path), so the two cannot disagree about what the number is; they can only
//! disagree about which build they are. The engine asks the helper for its
//! number before the first privileged step and refuses a helper that answers
//! another one, because a helper built from older source writes other paths
//! than the engine then verifies. Bump it whenever the helper's verbs, its
//! manifests, or the paths it writes change.
//!
//! History: 1 wrote `/etc/sddm.conf.d/90-omaboot.conf`; 2 writes
//! `zz-omaboot.conf` (so it sorts after every drop-in Omarchy and its
//! plugins install) and answers `protocol`.
pub const PROTOCOL: u32 = 2;
