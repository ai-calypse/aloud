//! AirDrop, through the system share sheet's own service.
//!
//! `NSSharingService` is AppKit, so it must be driven from the main thread,
//! and it shows macOS's own AirDrop picker — the user chooses the recipient
//! there; Aloud never sees or lists devices.
//!
//! ponytail: fire and forget. The service reports success and failure through
//! a delegate; without one a cancelled picker looks the same as a sent file.
//! Add a delegate (an `objc2` `declare_class!`) when the notch should say "Sent".

use std::path::PathBuf;

use anyhow::{bail, Result};

/// Opens the AirDrop picker for these files. Must run on the main thread
/// (the caller proves it by passing the marker's thread, see `src-tauri`).
#[cfg(target_os = "macos")]
pub fn share(paths: &[PathBuf]) -> Result<()> {
    use objc2::rc::Id;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{NSSharingService, NSSharingServiceNameSendViaAirDrop};
    use objc2_foundation::{MainThreadMarker, NSArray, NSString, NSURL};

    if paths.is_empty() {
        bail!("nothing to send");
    }
    let Some(_mtm) = MainThreadMarker::new() else {
        bail!("AirDrop has to be opened from the main thread");
    };
    let urls: Vec<Id<AnyObject>> = paths
        .iter()
        .map(|p| {
            let path = NSString::from_str(&p.to_string_lossy());
            // SAFETY: a plain constructor on a valid string.
            let url = unsafe { NSURL::fileURLWithPath(&path) };
            Id::into_super(Id::into_super(url))
        })
        .collect();
    let items = NSArray::from_vec(urls);
    // SAFETY: main thread (checked above); the array holds file URLs, which
    // the AirDrop service accepts.
    unsafe {
        let Some(service) =
            NSSharingService::sharingServiceNamed(NSSharingServiceNameSendViaAirDrop)
        else {
            bail!("AirDrop is not available on this Mac");
        };
        if !service.canPerformWithItems(Some(&items)) {
            bail!("AirDrop cannot send these files");
        }
        service.performWithItems(&items);
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn share(_paths: &[PathBuf]) -> Result<()> {
    bail!("AirDrop is a Mac feature")
}
