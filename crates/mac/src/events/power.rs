//! Charger edges from IOKit and Low Power Mode from NSProcessInfo.
//!
//! `IOPSNotificationCreateRunLoopSource` is the push replacement for polling
//! `pmset`: it fires when any power source changes, which includes every
//! percent the battery loses, so it also tells the app when to re-read the
//! battery ([`Event::BatteryMoved`]). The percent for the whisper comes from
//! the same IOPS snapshot, in-process, not a `pmset` spawn.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;
use std::time::Duration;

use block2::RcBlock;
use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop, CFRunLoopSource};
use core_foundation::string::CFString;
use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation_sys::base::{CFRelease, CFTypeRef};
use core_foundation_sys::dictionary::CFDictionaryRef;
use core_foundation_sys::runloop::CFRunLoopSourceRef;
use core_foundation_sys::string::CFStringRef;
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSProcessInfo,
    NSProcessInfoPowerStateDidChangeNotification,
};

use super::{Emit, Event, Settle};

/// Short: a plug seated badly flaps in well under this, and the whisper is
/// feedback for an act the user just made, so it should not lag.
const CHARGER_SETTLE: Duration = Duration::from_millis(400);

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPSNotificationCreateRunLoopSource(
        callback: extern "C" fn(*mut c_void),
        context: *mut c_void,
    ) -> CFRunLoopSourceRef;
    // Get rule. NULL snapshot is documented: it reads a fresh one.
    fn IOPSGetProvidingPowerSourceType(snapshot: CFTypeRef) -> CFStringRef;
    // Create rule.
    fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
    fn IOPSCopyPowerSourcesList(blob: CFTypeRef) -> CFArrayRef;
    // Get rule.
    fn IOPSGetPowerSourceDescription(blob: CFTypeRef, source: CFTypeRef) -> CFDictionaryRef;
}

/// "Battery Power" is the only source that is not a plug: AC, or a UPS.
pub fn is_plugged(source_type: &str) -> bool {
    source_type != "Battery Power"
}

/// Whether the Mac is on a plug right now. Public for the example.
pub fn plugged() -> bool {
    // SAFETY: a NULL snapshot is allowed; the result follows the Get rule and
    // is read at once, before anything can release it.
    let s = unsafe { IOPSGetProvidingPowerSourceType(std::ptr::null()) };
    if s.is_null() {
        return false;
    }
    // SAFETY: non-null, a CFString by the function's contract.
    is_plugged(&unsafe { CFString::wrap_under_get_rule(s) }.to_string())
}

/// The internal battery's charge, 0–100; None on a Mac without one.
pub fn battery_percent() -> Option<u8> {
    // SAFETY: each Copy result is released below on every path; each Get
    // result is used only while its owner is alive.
    unsafe {
        let blob = IOPSCopyPowerSourcesInfo();
        if blob.is_null() {
            return None;
        }
        let list = IOPSCopyPowerSourcesList(blob);
        let mut found = None;
        if !list.is_null() {
            for i in 0..CFArrayGetCount(list) {
                let source = CFArrayGetValueAtIndex(list, i);
                let d = IOPSGetPowerSourceDescription(blob, source);
                if d.is_null() {
                    continue;
                }
                let d: CFDictionary<CFString, CFType> = CFDictionary::wrap_under_get_rule(d);
                let int = |key: &str| {
                    d.find(CFString::new(key))
                        .and_then(|v| v.downcast::<CFNumber>())
                        .and_then(|n| n.to_i32())
                };
                if let (Some(now), Some(max)) = (int("Current Capacity"), int("Max Capacity")) {
                    found = percent(now, max);
                    break;
                }
            }
            CFRelease(list.cast());
        }
        CFRelease(blob);
        found
    }
}

/// `Current Capacity` over `Max Capacity` (both read 0–100 on Apple silicon,
/// mAh on older Macs, hence the division).
pub fn percent(now: i32, max: i32) -> Option<u8> {
    (max > 0).then(|| {
        (f64::from(now.max(0)) / f64::from(max) * 100.0)
            .round()
            .min(100.0) as u8
    })
}

struct Charger {
    emit: Emit,
    settle: Arc<Settle<bool>>,
}

extern "C" fn power_sources_changed(context: *mut c_void) {
    // SAFETY: the context is the leaked `Charger` from `install`, never freed.
    let c = unsafe { &*(context as *const Charger) };
    (c.emit)(Event::BatteryMoved);
    let emit = c.emit.clone();
    c.settle.observe(plugged(), move |_, now| {
        emit(Event::Charger {
            plugged: now,
            percent: battery_percent(),
        });
    });
}

pub fn install(emit: Emit) {
    let charger = Box::new(Charger {
        emit: emit.clone(),
        settle: Settle::new(plugged(), CHARGER_SETTLE),
    });
    // ponytail: leaked for the process's life — watchers are never uninstalled.
    let context = Box::into_raw(charger).cast();
    // SAFETY: the callback is `extern "C"` and the context outlives it (leaked).
    let source = unsafe { IOPSNotificationCreateRunLoopSource(power_sources_changed, context) };
    if source.is_null() {
        tracing::warn!("could not subscribe to power source changes");
    } else {
        // SAFETY: a Create-rule source, owned by the wrapper.
        let source = unsafe { CFRunLoopSource::wrap_under_create_rule(source) };
        // Common modes: the app's own loop modes (tracking, modal) must not
        // starve a charger edge.
        // SAFETY: a static constant.
        CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
    }

    low_power(emit);
}

pub fn low_power_on() -> bool {
    // SAFETY: a plain getter on the shared NSProcessInfo.
    unsafe { NSProcessInfo::processInfo().isLowPowerModeEnabled() }
}

/// `NSProcessInfoPowerStateDidChangeNotification` is posted on whichever
/// thread changed the state, so no run loop of ours is involved.
fn low_power(emit: Emit) {
    let settle = Settle::new(low_power_on(), Duration::from_millis(300));
    let block = RcBlock::new(move |_: NonNull<NSNotification>| {
        let emit = emit.clone();
        settle.observe(low_power_on(), move |_, on| emit(Event::LowPower(on)));
    });
    // SAFETY: the name is a valid static; a nil queue runs the block on the
    // posting thread; the observer is leaked below so the block stays alive.
    let observer = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSProcessInfoPowerStateDidChangeNotification),
            None,
            None,
            &block,
        )
    };
    // ponytail: leaked for the process's life, like the IOPS context.
    std::mem::forget(observer);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_battery_power_is_unplugged() {
        assert!(!is_plugged("Battery Power"));
        assert!(is_plugged("AC Power"));
        assert!(is_plugged("UPS Power"));
    }

    #[test]
    fn percent_survives_a_zero_max_and_mah_readings() {
        assert_eq!(percent(82, 100), Some(82));
        assert_eq!(percent(2310, 4629), Some(50));
        assert_eq!(percent(5, 0), None);
        assert_eq!(percent(5000, 4629), Some(100), "never over 100");
    }
}
