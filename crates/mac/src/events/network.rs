//! Wi-Fi and VPN from SystemConfiguration's dynamic store.
//!
//! One store, one notification: the same keys carry both. The Wi-Fi's SSID
//! would need Location access, so "joined" means the Wi-Fi interface holds an
//! IPv4 address and "off" means its radio's `Power Status` is false. A VPN is a
//! network service whose interface is a tunnel (`utun`, `ppp`, `ipsec`) with an
//! IPv4 address — which covers NetworkExtension apps, L2TP and IPSec alike,
//! where `SCNetworkConnection` would only see services macOS itself configured.
//! ponytail: an IPv6-only tunnel is not seen; read the `IPv6` keys too if one
//! turns up.

use std::ffi::c_void;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop, CFRunLoopSource};
use core_foundation::string::CFString;
use core_foundation_sys::array::CFArrayRef;
use core_foundation_sys::base::{CFAllocatorRef, CFIndex, CFTypeRef};
use core_foundation_sys::runloop::CFRunLoopSourceRef;
use core_foundation_sys::string::CFStringRef;

use super::{Emit, Event, Settle};

/// Long enough to ride out a roam or a sleep/wake rejoin, short enough that a
/// switch the user flipped still feels answered.
const WIFI_SETTLE: Duration = Duration::from_millis(2000);
const VPN_SETTLE: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wifi {
    Off,
    /// Radio on, no network.
    Searching,
    Joined,
}

/// The Wi-Fi interface name from a `Setup:/Network/Interface/<if>/AirPort` key.
pub fn wifi_interface(key: &str) -> Option<&str> {
    key.strip_prefix("Setup:/Network/Interface/")?
        .strip_suffix("/AirPort")
        .filter(|name| !name.contains('/'))
}

/// A tunnel interface, as opposed to a physical one.
pub fn is_tunnel(interface: &str) -> bool {
    ["utun", "ppp", "ipsec"]
        .iter()
        .any(|p| interface.starts_with(p))
}

/// What a Wi-Fi report becomes: only joining and switching off are news. A
/// drop to "searching" is expected to be followed by a join, which the settle
/// then reports as nothing at all if it came inside the delay.
pub fn wifi_event(from: Wifi, to: Wifi) -> Option<Event> {
    match (from, to) {
        (_, Wifi::Joined) => Some(Event::Wifi(true)),
        (Wifi::Off, _) => None,
        (_, Wifi::Off) => Some(Event::Wifi(false)),
        _ => None,
    }
}

#[link(name = "SystemConfiguration", kind = "framework")]
extern "C" {
    fn SCDynamicStoreCreate(
        allocator: CFAllocatorRef,
        name: CFStringRef,
        callout: Option<extern "C" fn(CFTypeRef, CFArrayRef, *mut c_void)>,
        context: *mut Context,
    ) -> CFTypeRef;
    fn SCDynamicStoreSetNotificationKeys(
        store: CFTypeRef,
        keys: CFArrayRef,
        patterns: CFArrayRef,
    ) -> u8;
    fn SCDynamicStoreCreateRunLoopSource(
        allocator: CFAllocatorRef,
        store: CFTypeRef,
        order: CFIndex,
    ) -> CFRunLoopSourceRef;
    // Create rule.
    fn SCDynamicStoreCopyKeyList(store: CFTypeRef, pattern: CFStringRef) -> CFArrayRef;
    // Create rule.
    fn SCDynamicStoreCopyValue(store: CFTypeRef, key: CFStringRef) -> CFTypeRef;
}

#[repr(C)]
struct Context {
    version: CFIndex,
    info: *mut c_void,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
}

fn keys(store: CFTypeRef, pattern: &str) -> Vec<String> {
    let pattern = CFString::new(pattern);
    // SAFETY: a Create-rule array, owned by the wrapper.
    let list = unsafe { SCDynamicStoreCopyKeyList(store, pattern.as_concrete_TypeRef()) };
    if list.is_null() {
        return Vec::new();
    }
    // SAFETY: just checked non-null; an array of CFStrings.
    let list: CFArray<CFString> = unsafe { CFArray::wrap_under_create_rule(list) };
    list.iter().map(|k| k.to_string()).collect()
}

fn value(store: CFTypeRef, key: &str) -> Option<CFDictionary<CFString, CFType>> {
    let key = CFString::new(key);
    // SAFETY: a Create-rule property list, owned by the wrapper once non-null.
    let v = unsafe { SCDynamicStoreCopyValue(store, key.as_concrete_TypeRef()) };
    if v.is_null() {
        return None;
    }
    // SAFETY: non-null. Every key read here holds a dictionary; anything else
    // is dropped by the type check.
    let v = unsafe { CFType::wrap_under_create_rule(v) };
    v.downcast_into::<CFDictionary>()
        // SAFETY: the same dictionary, re-typed by its (string) keys.
        .map(|d| unsafe { CFDictionary::wrap_under_get_rule(d.as_concrete_TypeRef()) })
}

fn text(d: &CFDictionary<CFString, CFType>, key: &str) -> Option<String> {
    d.find(CFString::new(key))
        .and_then(|v| v.downcast::<CFString>())
        .map(|s| s.to_string())
}

pub fn wifi_now(store: CFTypeRef) -> Wifi {
    let Some(name) = keys(store, "Setup:/Network/Interface/[^/]+/AirPort")
        .iter()
        .find_map(|k| wifi_interface(k).map(str::to_owned))
    else {
        // A Mac with no Wi-Fi: nothing to report, and `Off` never changes.
        return Wifi::Off;
    };
    let powered = value(store, &format!("State:/Network/Interface/{name}/AirPort"))
        .and_then(|d| {
            d.find(CFString::new("Power Status"))
                .and_then(|v| v.downcast::<CFBoolean>())
        })
        .is_some_and(bool::from);
    if !powered {
        Wifi::Off
    } else if value(store, &format!("State:/Network/Interface/{name}/IPv4")).is_some() {
        Wifi::Joined
    } else {
        Wifi::Searching
    }
}

pub fn vpn_now(store: CFTypeRef) -> bool {
    keys(store, "State:/Network/Service/[^/]+/IPv4")
        .iter()
        .filter_map(|k| value(store, k))
        .filter_map(|d| text(&d, "InterfaceName"))
        .any(|i| is_tunnel(&i))
}

struct Settles {
    wifi: Arc<Settle<Wifi>>,
    vpn: Arc<Settle<bool>>,
}

struct State {
    emit: Emit,
    /// Filled once the first reading is taken, which needs the store this
    /// state is the context of.
    settles: OnceLock<Settles>,
}

extern "C" fn changed(store: CFTypeRef, _keys: CFArrayRef, context: *mut c_void) {
    // SAFETY: the context is the leaked `State` from `install`, never freed.
    let s = unsafe { &*(context as *const State) };
    let Some(settles) = s.settles.get() else {
        return;
    };
    let emit = s.emit.clone();
    settles.wifi.observe(wifi_now(store), move |from, to| {
        if let Some(e) = wifi_event(from, to) {
            emit(e);
        }
    });
    let emit = s.emit.clone();
    settles
        .vpn
        .observe(vpn_now(store), move |_, on| emit(Event::Vpn(on)));
}

/// The current Wi-Fi and VPN state, for the example to print at rest.
pub fn snapshot() -> String {
    let name = CFString::new("aloud-notch-events-read");
    let mut ctx = Context {
        version: 0,
        info: std::ptr::null_mut(),
        retain: std::ptr::null(),
        release: std::ptr::null(),
        copy_description: std::ptr::null(),
    };
    // SAFETY: no callout, so the context is never used.
    let store = unsafe {
        SCDynamicStoreCreate(std::ptr::null(), name.as_concrete_TypeRef(), None, &mut ctx)
    };
    if store.is_null() {
        return "network store unavailable".into();
    }
    format!("wifi {:?}, vpn {}", wifi_now(store), vpn_now(store))
}

pub fn install(emit: Emit) {
    // ponytail: leaked for the process's life — watchers are never uninstalled.
    let state: &'static State = Box::leak(Box::new(State {
        emit,
        settles: OnceLock::new(),
    }));
    let mut ctx = Context {
        version: 0,
        info: (state as *const State).cast_mut().cast(),
        retain: std::ptr::null(),
        release: std::ptr::null(),
        copy_description: std::ptr::null(),
    };
    let name = CFString::new("aloud-notch-events");
    // SAFETY: the callback is `extern "C"`; the context is copied by the call
    // and its `info` is the leaked state.
    let store = unsafe {
        SCDynamicStoreCreate(
            std::ptr::null(),
            name.as_concrete_TypeRef(),
            Some(changed),
            &mut ctx,
        )
    };
    if store.is_null() {
        tracing::warn!("could not open the network store");
        return;
    }
    let _ = state.settles.set(Settles {
        wifi: Settle::new(wifi_now(store), WIFI_SETTLE),
        vpn: Settle::new(vpn_now(store), VPN_SETTLE),
    });
    let patterns = CFArray::from_CFTypes(&[
        CFString::new("State:/Network/Interface/[^/]+/AirPort"),
        CFString::new("State:/Network/Interface/[^/]+/IPv4"),
        CFString::new("State:/Network/Service/[^/]+/IPv4"),
    ]);
    // SAFETY: both arguments are live CFArrays; the store is non-null below.
    let ok = !store.is_null()
        && unsafe {
            SCDynamicStoreSetNotificationKeys(
                store,
                std::ptr::null(),
                patterns.as_concrete_TypeRef(),
            )
        } != 0;
    // SAFETY: a Create-rule source over a live store.
    let source = if ok {
        unsafe { SCDynamicStoreCreateRunLoopSource(std::ptr::null(), store, 0) }
    } else {
        std::ptr::null_mut()
    };
    if source.is_null() {
        tracing::warn!("could not subscribe to network changes");
        return;
    }
    // SAFETY: Create rule, non-null; the store is kept alive by the source.
    let source = unsafe { CFRunLoopSource::wrap_under_create_rule(source) };
    // SAFETY: a static constant.
    CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wifi_interface_is_named_by_its_setup_key_not_awdl() {
        assert_eq!(
            wifi_interface("Setup:/Network/Interface/en0/AirPort"),
            Some("en0")
        );
        assert_eq!(wifi_interface("State:/Network/Interface/en0/AirPort"), None);
        assert_eq!(wifi_interface("Setup:/Network/Interface/a/b/AirPort"), None);
    }

    #[test]
    fn tunnels_are_vpns_and_ethernet_is_not() {
        for t in ["utun4", "ppp0", "ipsec0"] {
            assert!(is_tunnel(t), "{t}");
        }
        for p in ["en0", "en5", "bridge0", "awdl0"] {
            assert!(!is_tunnel(p), "{p}");
        }
    }

    #[test]
    fn joining_and_switching_off_are_news_and_searching_is_not() {
        assert_eq!(wifi_event(Wifi::Off, Wifi::Joined), Some(Event::Wifi(true)));
        assert_eq!(
            wifi_event(Wifi::Searching, Wifi::Joined),
            Some(Event::Wifi(true))
        );
        assert_eq!(
            wifi_event(Wifi::Joined, Wifi::Off),
            Some(Event::Wifi(false))
        );
        assert_eq!(wifi_event(Wifi::Joined, Wifi::Searching), None);
        assert_eq!(
            wifi_event(Wifi::Searching, Wifi::Off),
            Some(Event::Wifi(false))
        );
        assert_eq!(wifi_event(Wifi::Off, Wifi::Searching), None);
    }
}
