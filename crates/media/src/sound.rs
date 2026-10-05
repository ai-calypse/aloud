//! Audio devices through CoreAudio: list the outputs and inputs, make one the
//! default, mute the default microphone, move the output volume the way the
//! volume keys do, and tell when the microphone is muted from outside.
//!
//! The FFI is declared by hand (a dozen lines) rather than through
//! `coreaudio-sys`, whose bindgen build costs minutes to read five properties.
//!
//! ponytail: no per-app Sound mixer. Per-app volume and EQ need Core Audio
//! process taps (macOS 14.4+) feeding an aggregate device, which is an audio
//! engine in its own right.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// CoreAudio's id: good for this boot, which is as long as a list lives.
    pub id: u32,
    pub name: String,
    /// What it is plugged in as: "Built-in", "Bluetooth", "USB", "HDMI"…
    pub transport: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SoundState {
    pub outputs: Vec<Device>,
    pub inputs: Vec<Device>,
    /// None when the default input has neither a mute switch nor a level to
    /// pull to zero (some USB interfaces): the view hides the switch.
    pub mic_muted: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Output,
    Input,
}

pub use imp::{
    has_output_volume, mic_muted, nudge_output, output, set_default, set_mic_muted, set_output,
    state, watch_mic,
};

/// What a volume key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeAction {
    Up,
    Down,
    ToggleMute,
}

/// The output as the HUD reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Output {
    /// 0.0–1.0, the level the slider would show.
    pub level: f32,
    pub muted: bool,
}

impl Output {
    /// 0–100 for the bar: a muted output reads empty, as macOS draws it.
    pub fn percent(self) -> u8 {
        if self.muted {
            0
        } else {
            (self.level.clamp(0.0, 1.0) * 100.0).round() as u8
        }
    }
}

/// One key press, from `from`. A step is 1/16, or 1/64 with Option+Shift, and
/// lands on the grid: from 0.53 one press up is 0.5625, as macOS does, not
/// 0.5925. Either arrow also unmutes.
pub fn step_output(from: Output, action: VolumeAction, fine: bool) -> Output {
    let step = if fine { 1.0 / 64.0 } else { 1.0 / 16.0 };
    // A hair of slack so a level already on the grid (stored as an f32) moves
    // a whole step instead of snapping to itself.
    const SLACK: f32 = 1e-3;
    let slots = from.level / step;
    match action {
        VolumeAction::Up => Output {
            level: (((slots + SLACK).floor() + 1.0) * step).min(1.0),
            muted: false,
        },
        VolumeAction::Down => Output {
            level: (((slots - SLACK).ceil() - 1.0) * step).max(0.0),
            muted: false,
        },
        VolumeAction::ToggleMute => Output {
            muted: !from.muted,
            ..from
        },
    }
}

/// A four-character code as CoreAudio spells selectors and scopes.
const fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

/// How a CoreAudio transport code reads in a menu.
fn transport_name(code: u32) -> &'static str {
    match &code.to_be_bytes() {
        b"bltn" => "Built-in",
        b"blue" | b"blea" => "Bluetooth",
        b"usb " => "USB",
        b"hdmi" | b"dprt" => "Display",
        b"airp" => "AirPlay",
        b"virt" => "Virtual",
        b"aggr" => "Aggregate",
        _ => "",
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use anyhow::{bail, Context, Result};
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::sync::{Arc, Mutex};

    type OsStatus = i32;
    type ObjectId = u32;

    #[repr(C)]
    struct Address {
        selector: u32,
        scope: u32,
        element: u32,
    }

    #[link(name = "CoreAudio", kind = "framework")]
    unsafe extern "C" {
        fn AudioObjectGetPropertyDataSize(
            id: ObjectId,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
        ) -> OsStatus;
        fn AudioObjectGetPropertyData(
            id: ObjectId,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> OsStatus;
        fn AudioObjectSetPropertyData(
            id: ObjectId,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: u32,
            data: *const c_void,
        ) -> OsStatus;
        fn AudioObjectIsPropertySettable(
            id: ObjectId,
            address: *const Address,
            settable: *mut u8,
        ) -> OsStatus;
        fn AudioObjectAddPropertyListener(
            id: ObjectId,
            address: *const Address,
            listener: Listener,
            client: *mut c_void,
        ) -> OsStatus;
        fn AudioObjectRemovePropertyListener(
            id: ObjectId,
            address: *const Address,
            listener: Listener,
            client: *mut c_void,
        ) -> OsStatus;
    }

    type Listener = unsafe extern "C" fn(
        id: ObjectId,
        count: u32,
        addresses: *const Address,
        client: *mut c_void,
    ) -> OsStatus;

    const SYSTEM: ObjectId = 1;
    const GLOBAL: u32 = fourcc(b"glob");
    const OUTPUT: u32 = fourcc(b"outp");
    const INPUT: u32 = fourcc(b"inpt");
    const DEVICES: u32 = fourcc(b"dev#");
    const DEFAULT_OUT: u32 = fourcc(b"dOut");
    const DEFAULT_IN: u32 = fourcc(b"dIn ");
    const DEFAULT_SYSTEM_OUT: u32 = fourcc(b"sOut");
    const STREAMS: u32 = fourcc(b"stm#");
    const NAME: u32 = fourcc(b"lnam");
    const TRANSPORT: u32 = fourcc(b"tran");
    const CAN_BE_DEFAULT: u32 = fourcc(b"dflt");
    const MUTE: u32 = fourcc(b"mute");
    const VOLUME: u32 = fourcc(b"volm");
    /// `kAudioHardwareServiceDeviceProperty_VirtualMainVolume`: the one level
    /// the Sound slider and the volume keys move, whatever the channels do.
    const VIRTUAL_MAIN_VOLUME: u32 = fourcc(b"vmvc");

    fn addr(selector: u32, scope: u32) -> Address {
        Address {
            selector,
            scope,
            element: 0,
        }
    }

    fn check(status: OsStatus, what: &str) -> Result<()> {
        if status == 0 {
            Ok(())
        } else {
            bail!("CoreAudio could not {what} (status {status})")
        }
    }

    /// A fixed-size property: a `u32`, an `f32`.
    fn get<T: Default + Copy>(id: ObjectId, a: &Address) -> Result<T> {
        let mut value = T::default();
        let mut size = size_of::<T>() as u32;
        // SAFETY: `value` is a live `T` of `size` bytes, and the property types
        // this is called with (u32 ids, flags and f32 levels) are plain data.
        check(
            unsafe {
                AudioObjectGetPropertyData(
                    id,
                    a,
                    0,
                    std::ptr::null(),
                    &mut size,
                    (&mut value as *mut T).cast(),
                )
            },
            "read a property",
        )?;
        Ok(value)
    }

    fn set<T: Copy>(id: ObjectId, a: &Address, value: T) -> Result<()> {
        // SAFETY: `value` is a live `T` of the stated size, read once.
        check(
            unsafe {
                AudioObjectSetPropertyData(
                    id,
                    a,
                    0,
                    std::ptr::null(),
                    size_of::<T>() as u32,
                    (&value as *const T).cast(),
                )
            },
            "set a property",
        )
    }

    fn settable(id: ObjectId, a: &Address) -> bool {
        let mut yes = 0u8;
        // SAFETY: `yes` is a live byte the call writes a Boolean into.
        unsafe { AudioObjectIsPropertySettable(id, a, &mut yes) == 0 && yes != 0 }
    }

    fn size_of_property(id: ObjectId, a: &Address) -> u32 {
        let mut size = 0u32;
        // SAFETY: `size` is a live u32 the call writes into.
        let status =
            unsafe { AudioObjectGetPropertyDataSize(id, a, 0, std::ptr::null(), &mut size) };
        if status == 0 {
            size
        } else {
            0
        }
    }

    fn device_ids() -> Result<Vec<ObjectId>> {
        let a = addr(DEVICES, GLOBAL);
        let mut size = size_of_property(SYSTEM, &a);
        let mut ids = vec![0u32; size as usize / size_of::<u32>()];
        // SAFETY: `ids` holds `size` bytes of u32 slots for the call to fill.
        check(
            unsafe {
                AudioObjectGetPropertyData(
                    SYSTEM,
                    &a,
                    0,
                    std::ptr::null(),
                    &mut size,
                    ids.as_mut_ptr().cast(),
                )
            },
            "list the devices",
        )?;
        ids.truncate(size as usize / size_of::<u32>());
        Ok(ids)
    }

    fn name(id: ObjectId) -> Option<String> {
        let a = addr(NAME, GLOBAL);
        let mut cf: CFStringRef = std::ptr::null();
        let mut size = size_of::<CFStringRef>() as u32;
        // SAFETY: the call writes a retained CFStringRef into `cf`.
        let status = unsafe {
            AudioObjectGetPropertyData(
                id,
                &a,
                0,
                std::ptr::null(),
                &mut size,
                (&mut cf as *mut CFStringRef).cast(),
            )
        };
        // SAFETY: on success `cf` is a +1 reference this takes ownership of
        // (the "Get" in the name is CoreAudio's, the retain is the caller's).
        (status == 0 && !cf.is_null())
            .then(|| unsafe { CFString::wrap_under_create_rule(cf) }.to_string())
    }

    fn scope(direction: Direction) -> u32 {
        match direction {
            Direction::Output => OUTPUT,
            Direction::Input => INPUT,
        }
    }

    fn default_id(direction: Direction) -> Option<ObjectId> {
        let selector = match direction {
            Direction::Output => DEFAULT_OUT,
            Direction::Input => DEFAULT_IN,
        };
        get::<u32>(SYSTEM, &addr(selector, GLOBAL)).ok()
    }

    /// Devices with a stream in `direction` that the user may pick. The
    /// "can be default" flag is what hides the aggregate and loopback devices
    /// other apps leave behind, which the Sound menu hides too.
    fn devices(direction: Direction) -> Vec<Device> {
        let default = default_id(direction);
        device_ids()
            .unwrap_or_default()
            .into_iter()
            .filter(|&id| size_of_property(id, &addr(STREAMS, scope(direction))) > 0)
            .filter(|&id| {
                get::<u32>(id, &addr(CAN_BE_DEFAULT, scope(direction))).is_ok_and(|v| v != 0)
            })
            .filter_map(|id| {
                Some(Device {
                    id,
                    name: name(id)?,
                    transport: get::<u32>(id, &addr(TRANSPORT, GLOBAL))
                        .map(transport_name)
                        .unwrap_or_default()
                        .to_string(),
                    is_default: Some(id) == default,
                })
            })
            .collect()
    }

    pub fn state() -> SoundState {
        SoundState {
            outputs: devices(Direction::Output),
            inputs: devices(Direction::Input),
            mic_muted: mic_muted(),
        }
    }

    /// Makes `id` the default. An output also becomes the system-sound output,
    /// as the Sound menu does, or alerts keep playing from the old speakers.
    pub fn set_default(direction: Direction, id: u32) -> Result<()> {
        match direction {
            Direction::Output => {
                set(SYSTEM, &addr(DEFAULT_OUT, GLOBAL), id)?;
                set(SYSTEM, &addr(DEFAULT_SYSTEM_OUT, GLOBAL), id)
            }
            Direction::Input => set(SYSTEM, &addr(DEFAULT_IN, GLOBAL), id),
        }
    }

    /// Where the pre-mute level lives, for the mics that have no mute switch.
    fn restore_path() -> Result<std::path::PathBuf> {
        Ok(aloud_notes::data_dir()?.join("mic-restore"))
    }

    /// The mic's mute switch if it has one, else its input level (the built-in
    /// mic has the level only); `None` for neither.
    fn mic_control(id: ObjectId) -> Option<(Address, bool)> {
        let mute = addr(MUTE, INPUT);
        if settable(id, &mute) {
            return Some((mute, true));
        }
        let volume = addr(VOLUME, INPUT);
        settable(id, &volume).then_some((volume, false))
    }

    fn output_address() -> Address {
        addr(VIRTUAL_MAIN_VOLUME, OUTPUT)
    }

    fn read_output() -> Result<(ObjectId, Output)> {
        let id = default_id(Direction::Output).context("no default output")?;
        let level = get::<f32>(id, &output_address())?;
        // Not every output has a mute switch; one without is never "muted".
        let muted = get::<u32>(id, &addr(MUTE, OUTPUT)).is_ok_and(|v| v != 0);
        Ok((id, Output { level, muted }))
    }

    /// The default output's level and mute, as the keys would find them.
    pub fn output() -> Result<Output> {
        read_output().map(|(_, o)| o)
    }

    /// Puts the default output back to `to`: for the examples, which move the
    /// volume to prove a key and must leave it as they found it.
    pub fn set_output(to: Output) -> Result<()> {
        let (id, _) = read_output()?;
        set(id, &output_address(), to.level)?;
        let mute = addr(MUTE, OUTPUT);
        if settable(id, &mute) {
            set(id, &mute, u32::from(to.muted))?;
        }
        Ok(())
    }

    /// Whether the default output has a volume the keys can move. HDMI and
    /// some USB interfaces do not: macOS greys the slider out for them.
    pub fn has_output_volume() -> bool {
        default_id(Direction::Output).is_some_and(|id| settable(id, &output_address()))
    }

    /// Presses a volume key on the default output and says where it ended up.
    /// Blocking CoreAudio calls: never on the event tap's thread.
    ///
    /// ponytail: an output with a volume but no mute switch cannot be muted by
    /// the key (macOS pulls the level to zero and remembers it); the key then
    /// does nothing rather than half of that.
    pub fn nudge_output(action: VolumeAction, fine: bool) -> Result<Output> {
        let (id, now) = read_output()?;
        let next = step_output(now, action, fine);
        if next.level != now.level {
            set(id, &output_address(), next.level)?;
        }
        if next.muted != now.muted {
            let mute = addr(MUTE, OUTPUT);
            if !settable(id, &mute) {
                bail!("this output has no mute switch");
            }
            set(id, &mute, u32::from(next.muted))?;
        }
        Ok(next)
    }

    pub fn mic_muted() -> Option<bool> {
        let id = default_id(Direction::Input)?;
        let (a, is_switch) = mic_control(id)?;
        if is_switch {
            get::<u32>(id, &a).ok().map(|v| v != 0)
        } else {
            get::<f32>(id, &a).ok().map(|v| v <= 0.0)
        }
    }

    /// Silences every app's microphone at once, at the device. A level-only
    /// mic is pulled to zero with the old level saved to disk *before*, so a
    /// crash while muted leaves a file that unmute reads, not a mic stuck
    /// at zero with nothing to undo it.
    pub fn set_mic_muted(muted: bool) -> Result<()> {
        let id = default_id(Direction::Input).context("no default microphone")?;
        let (a, is_switch) = mic_control(id).context("this microphone cannot be muted")?;
        if is_switch {
            return set(id, &a, u32::from(muted));
        }
        let path = restore_path()?;
        if muted {
            let level = get::<f32>(id, &a)?;
            // Already at zero: keep the earlier saved level, or unmuting would
            // restore silence.
            if level > 0.0 {
                std::fs::write(&path, level.to_string()).context("saving the mic level")?;
            }
            set(id, &a, 0.0f32)
        } else {
            let saved = std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| s.trim().parse::<f32>().ok())
                .filter(|v| *v > 0.0)
                .unwrap_or(0.75);
            set(id, &a, saved)?;
            let _ = std::fs::remove_file(&path);
            Ok(())
        }
    }

    /// What the microphone watch tells, and the last thing it saw.
    struct MicWatch {
        on_change: Arc<dyn Fn(bool) + Send + Sync>,
        /// The device the listeners sit on right now.
        device: Option<ObjectId>,
        muted: Option<bool>,
    }

    static MIC_WATCH: Mutex<Option<MicWatch>> = Mutex::new(None);

    fn mic_watch() -> std::sync::MutexGuard<'static, Option<MicWatch>> {
        MIC_WATCH.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The two ways a mic reads as muted: its switch, or (the built-in mic has
    /// no switch) its level.
    fn mic_addresses() -> [Address; 2] {
        [addr(MUTE, INPUT), addr(VOLUME, INPUT)]
    }

    /// A mute or level change on the default input.
    unsafe extern "C" fn on_mic_property(
        _id: ObjectId,
        _count: u32,
        _addresses: *const Address,
        _client: *mut c_void,
    ) -> OsStatus {
        let now = mic_muted();
        let tell = {
            let mut guard = mic_watch();
            let Some(watch) = guard.as_mut() else {
                return 0;
            };
            // A level dragged about above zero is not news; only the edge is.
            now.filter(|&n| watch.muted != Some(n)).map(|n| {
                watch.muted = Some(n);
                (n, watch.on_change.clone())
            })
        };
        // Outside the lock: the callback may take its time.
        if let Some((muted, on_change)) = tell {
            on_change(muted);
        }
        0
    }

    /// The default input changed: move the listeners to the new one.
    unsafe extern "C" fn on_default_input(
        _id: ObjectId,
        _count: u32,
        _addresses: *const Address,
        _client: *mut c_void,
    ) -> OsStatus {
        // Off the notification: CoreAudio can deadlock a listener that adds or
        // removes listeners while it is being called. Rare, so a thread each.
        std::thread::spawn(attach_mic);
        0
    }

    /// Listeners on the current default input, replacing those on the last. A
    /// device change is not a mute: the new device's state is only recorded.
    fn attach_mic() {
        let mut guard = mic_watch();
        let Some(watch) = guard.as_mut() else { return };
        let id = default_id(Direction::Input);
        if watch.device == id {
            return;
        }
        if let Some(old) = watch.device.take() {
            for a in mic_addresses() {
                // SAFETY: the same id, address and callback it was added with.
                unsafe {
                    AudioObjectRemovePropertyListener(
                        old,
                        &a,
                        on_mic_property,
                        std::ptr::null_mut(),
                    )
                };
            }
        }
        if let Some(new) = id {
            for a in mic_addresses() {
                // SAFETY: `a` is read during the call; the callback is a plain
                // function and takes no client data.
                unsafe {
                    AudioObjectAddPropertyListener(new, &a, on_mic_property, std::ptr::null_mut())
                };
            }
        }
        watch.device = id;
        watch.muted = mic_muted();
    }

    /// Calls `on_change(muted)` whenever the default microphone is muted or
    /// unmuted by anyone: the Sound pane, another app, or [`set_mic_muted`].
    /// Follows the default input across a device change. Call once.
    ///
    /// CoreAudio delivers these on its own notification thread, so no run loop
    /// is needed (the process's HAL run loop is left unset, which is its
    /// default); `on_change` runs there and must return quickly.
    pub fn watch_mic(on_change: impl Fn(bool) + Send + Sync + 'static) -> Result<()> {
        {
            let mut guard = mic_watch();
            if guard.is_some() {
                bail!("the microphone is already watched");
            }
            *guard = Some(MicWatch {
                on_change: Arc::new(on_change),
                device: None,
                muted: None,
            });
        }
        attach_mic();
        let a = addr(DEFAULT_IN, GLOBAL);
        // SAFETY: as above; the system object lives for the process.
        check(
            unsafe {
                AudioObjectAddPropertyListener(SYSTEM, &a, on_default_input, std::ptr::null_mut())
            },
            "watch the default input",
        )
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::*;
    use anyhow::{bail, Result};

    // ponytail: Windows needs IMMDeviceEnumerator and IPolicyConfig (the latter
    // undocumented); written once a Windows user exists to test it.
    pub fn state() -> SoundState {
        SoundState::default()
    }
    pub fn set_default(_: Direction, _: u32) -> Result<()> {
        bail!("audio devices are macOS-only for now")
    }
    pub fn mic_muted() -> Option<bool> {
        None
    }
    pub fn set_mic_muted(_: bool) -> Result<()> {
        bail!("audio devices are macOS-only for now")
    }
    pub fn has_output_volume() -> bool {
        false
    }
    pub fn nudge_output(_: VolumeAction, _: bool) -> Result<Output> {
        bail!("audio devices are macOS-only for now")
    }
    pub fn output() -> Result<Output> {
        bail!("audio devices are macOS-only for now")
    }
    pub fn set_output(_: Output) -> Result<()> {
        bail!("audio devices are macOS-only for now")
    }
    pub fn watch_mic(_: impl Fn(bool) + Send + Sync + 'static) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards: CoreAudio's codes are big-endian four-character literals, and
    /// a little-endian read makes every property "not found".
    #[test]
    fn fourcc_is_big_endian() {
        assert_eq!(fourcc(b"dOut"), 0x644F_7574);
        assert_eq!(fourcc(b"glob"), 0x676C_6F62);
    }

    fn out(level: f32, muted: bool) -> Output {
        Output { level, muted }
    }

    /// The failure this guards: stepping by a fixed 1/16 off the current level,
    /// which drifts off the grid macOS keeps (8 presses up from 0.53 must end
    /// on a notch, not between two).
    #[test]
    fn a_volume_step_lands_on_the_grid_macos_uses() {
        let up = |l| step_output(out(l, false), VolumeAction::Up, false).level;
        let down = |l| step_output(out(l, false), VolumeAction::Down, false).level;
        assert_eq!(up(0.53), 0.5625);
        assert_eq!(up(0.5), 0.5625, "on the grid already: a whole step");
        assert_eq!(down(0.53), 0.5);
        assert_eq!(down(0.5), 0.4375);
        assert_eq!(up(1.0), 1.0);
        assert_eq!(down(0.0), 0.0);
    }

    #[test]
    fn option_shift_moves_a_quarter_step() {
        let next = step_output(out(0.5, false), VolumeAction::Up, true);
        assert_eq!(next.level, 0.5 + 1.0 / 64.0);
    }

    #[test]
    fn either_arrow_unmutes_and_the_mute_key_toggles() {
        assert!(!step_output(out(0.5, true), VolumeAction::Up, false).muted);
        assert!(!step_output(out(0.5, true), VolumeAction::Down, false).muted);
        let toggled = step_output(out(0.5, false), VolumeAction::ToggleMute, false);
        assert!(toggled.muted);
        assert_eq!(toggled.level, 0.5, "mute keeps the level to come back to");
    }

    #[test]
    fn a_muted_output_reads_empty_in_the_hud() {
        assert_eq!(out(0.5, true).percent(), 0);
        assert_eq!(out(0.5, false).percent(), 50);
        assert_eq!(out(1.4, false).percent(), 100);
    }

    #[test]
    fn transports_read_as_words() {
        assert_eq!(transport_name(fourcc(b"bltn")), "Built-in");
        assert_eq!(transport_name(fourcc(b"blue")), "Bluetooth");
        assert_eq!(transport_name(fourcc(b"usb ")), "USB");
        assert_eq!(transport_name(fourcc(b"zzzz")), "");
    }
}
