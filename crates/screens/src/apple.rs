//! The `NSScreen` side. Main thread only, except [`mouse_location`].

use std::ptr::NonNull;
use std::sync::Arc;

use block2::RcBlock;
use objc2_app_kit::{NSApplicationDidChangeScreenParametersNotification, NSEvent, NSScreen};
use objc2_foundation::{MainThreadMarker, NSNotification, NSNotificationCenter};

use crate::{store, Notch, Rect, Screen};

/// Re-reads every display into [`crate::current`]. Main thread: off it AppKit
/// is not ours to touch, so this is a no-op there rather than a crash.
pub fn refresh() {
    let Some(mtm) = MainThreadMarker::new() else {
        tracing::warn!("screens refreshed off the main thread; skipped");
        return;
    };
    let mut out = Vec::new();
    let screens = NSScreen::screens(mtm);
    for i in 0..screens.count() {
        let screen = unsafe { screens.objectAtIndex(i) };
        let frame = screen.frame();
        // safeAreaInsets.top is the notch height, and 0 without one — the
        // cheapest "has a notch?" test AppKit offers.
        let top = unsafe { screen.safeAreaInsets() }.top;
        let notch = (top > 0.0).then(|| {
            // The notch is the strip the two menu-bar auxiliary areas leave open.
            let left = unsafe { screen.auxiliaryTopLeftArea() };
            let right = unsafe { screen.auxiliaryTopRightArea() };
            Notch {
                width: (frame.size.width - left.size.width - right.size.width).max(0.0),
                height: top,
            }
        });
        out.push(Screen {
            name: unsafe { screen.localizedName() }.to_string(),
            frame: Rect {
                x: frame.origin.x,
                y: frame.origin.y,
                w: frame.size.width,
                h: frame.size.height,
            },
            scale: screen.backingScaleFactor(),
            notch,
        });
    }
    store(out);
}

/// The pointer in the same space as the screen frames. A plain class method,
/// which tao itself calls from any thread for its own cursor reads.
pub fn mouse_location() -> Option<(f64, f64)> {
    // SAFETY: a stateless class method returning a point.
    let p = unsafe { NSEvent::mouseLocation() };
    Some((p.x, p.y))
}

/// Calls `on_change` after every screen-parameter change — a display plugged
/// in, removed, re-arranged, or a resolution switched — having already
/// [`refresh`]ed. Posted on the main thread, which is what `refresh` needs.
/// Install once, from the main thread. One change posts several times, so the
/// caller debounces whatever is expensive.
pub fn watch(on_change: impl Fn() + Send + Sync + 'static) {
    let on_change = Arc::new(on_change);
    let block = RcBlock::new(move |_: NonNull<NSNotification>| {
        refresh();
        on_change();
    });
    // SAFETY: the name is a valid static; a nil queue runs the block on the
    // posting thread; the observer is leaked below so the block stays alive.
    let observer = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSApplicationDidChangeScreenParametersNotification),
            None,
            None,
            &block,
        )
    };
    // ponytail: leaked for the process's life — watchers are never uninstalled.
    std::mem::forget(observer);
}
