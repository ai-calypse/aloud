//! Live check of the keys the notch answers: volume, brightness, and Caps Lock.
//!
//!   cargo run -p aloud-hotkey --example keys -- [seconds] [--replace]
//!
//! Prints every volume and brightness key press and Caps Lock change the tap
//! reports, for `seconds` (default 15). Without `--replace` it only watches:
//! macOS handles the keys and draws its own HUD. With it the tap eats the
//! volume and brightness keys, this moves the output volume and the built-in
//! display's brightness one step per press as the app does, and the level the
//! app would draw is printed. Both are restored on exit. Needs Accessibility.
//! Press the real keys, or drive it with `--example post_key`.

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    use aloud_hotkey::aux::{self, AuxEvent, BrightnessKey, VolumeKey};
    use aloud_hotkey::model::Config;
    use aloud_media::sound::{self, VolumeAction};
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    let args: Vec<String> = std::env::args().skip(1).collect();
    let seconds: u64 = args.iter().find_map(|a| a.parse().ok()).unwrap_or(15);
    let replace = args.iter().any(|a| a == "--replace");

    if !aloud_hotkey::is_permitted() {
        eprintln!("Accessibility permission not granted (System Settings > Privacy & Security).");
        std::process::exit(1);
    }

    let events = aux::events().expect("first caller");
    aux::set_replace_volume(replace);
    aux::set_replace_brightness(replace);
    aux::set_brightness_probe(aloud_mac::brightness::controllable);
    let before = sound::output()?;
    let bright_before = aloud_mac::brightness::get().ok();
    println!(
        "output volume {before:?}; brightness {bright_before:?} (controllable {}); replace={replace}; listening {seconds}s",
        aloud_mac::brightness::controllable()
    );

    std::thread::spawn(|| {
        let _ = aloud_hotkey::listen(
            Arc::new(Mutex::new(Config::defaults())),
            Arc::new(AtomicBool::new(false)),
            Default::default(),
            |_, _| {},
        );
    });

    // A listen-only tap *behind* the app's: it hears an event only if the app's
    // tap let it through, which is the proof of a swallow.
    let downstream = Arc::new(AtomicU32::new(0));
    probe(downstream.clone());

    let end = Instant::now() + Duration::from_secs(seconds);
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        let Ok(event) = events.recv_timeout(left) else {
            break;
        };
        println!("{event:?}");
        if let AuxEvent::Brightness {
            key,
            fine,
            swallowed: true,
        } = event
        {
            let was = aloud_mac::brightness::get()?;
            let now = aloud_mac::brightness::nudge(key == BrightnessKey::Up, fine)?;
            println!(
                "  swallowed; moved {was:.4} -> {now:.4}; HUD level {}",
                aloud_mac::brightness::percent(now)
            );
        }
        if let AuxEvent::Volume {
            key,
            fine,
            swallowed: true,
        } = event
        {
            let action = match key {
                VolumeKey::Up => VolumeAction::Up,
                VolumeKey::Down => VolumeAction::Down,
                VolumeKey::Mute => VolumeAction::ToggleMute,
            };
            let was = sound::output()?;
            let now = sound::nudge_output(action, fine)?;
            println!(
                "  swallowed; moved {:.4} -> {:.4} (muted {} -> {}); HUD level {}",
                was.level,
                now.level,
                was.muted,
                now.muted,
                now.percent()
            );
        }
    }

    println!(
        "downstream tap heard {} system-defined or key events (0 = swallowed; a press is a down and an up; your own typing counts)",
        downstream.load(Ordering::SeqCst)
    );
    // Volume keys and Caps Lock are not typing: this stays None unless a
    // letter was pressed during the run.
    println!(
        "since last keystroke: {:?}",
        aloud_hotkey::since_last_keystroke()
    );
    sound::set_output(before)?;
    println!("restored {:?}", sound::output()?);
    if let Some(level) = bright_before {
        aloud_mac::brightness::set(level)?;
        println!("restored brightness {:.4}", aloud_mac::brightness::get()?);
    }
    Ok(())
}

/// Counts NX_SYSDEFINED events that reach the end of the session's tap chain.
#[cfg(target_os = "macos")]
fn probe(count: std::sync::Arc<std::sync::atomic::AtomicU32>) {
    use std::ffi::c_void;
    use std::sync::{atomic::Ordering, Arc};

    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            mask: u64,
            callback: extern "C" fn(*const c_void, u32, *mut c_void, *mut c_void) -> *mut c_void,
            user: *mut c_void,
        ) -> *mut c_void;
        fn CGEventTapEnable(tap: *const c_void, enable: bool);
    }
    extern "C" fn heard(
        _: *const c_void,
        _: u32,
        event: *mut c_void,
        user: *mut c_void,
    ) -> *mut c_void {
        // SAFETY: `user` is the Arc leaked below.
        unsafe { &*user.cast::<std::sync::atomic::AtomicU32>() }.fetch_add(1, Ordering::SeqCst);
        event
    }

    std::thread::spawn(move || {
        use core_foundation::base::TCFType;
        use core_foundation::mach_port::CFMachPort;
        use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
        let user = Arc::as_ptr(&count) as *mut c_void;
        // Session, tail-append, listen-only: NX_SYSDEFINED and key down/up
        // (brightness can arrive as keycodes 144/145).
        let port = unsafe { CGEventTapCreate(1, 1, 1, 1 << 14 | 1 << 10 | 1 << 11, heard, user) };
        assert!(!port.is_null(), "probe tap");
        let port = unsafe { CFMachPort::wrap_under_create_rule(port.cast()) };
        let source = port.create_runloop_source(0).unwrap();
        unsafe { CFRunLoop::get_current().add_source(&source, kCFRunLoopCommonModes) };
        unsafe { CGEventTapEnable(port.as_concrete_TypeRef().cast(), true) };
        // `count` is kept alive by this closure for as long as the loop runs.
        let _keep = count;
        CFRunLoop::run_current();
    });
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("keys: macOS only");
}
