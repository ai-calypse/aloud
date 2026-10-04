//! Colours: reading one out of text, writing it back as hex or `rgb()`, and
//! the system eyedropper.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// `#rrggbb`, lowercase.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// `rgb(r, g, b)`, what CSS takes.
    pub fn css(self) -> String {
        format!("rgb({}, {}, {})", self.r, self.g, self.b)
    }
}

/// Reads `#rgb`, `#rrggbb` or `rgb(r, g, b)` — and only that: the whole text
/// must be the colour, so a sentence that mentions one is not offered a swatch.
pub fn parse(text: &str) -> Option<Rgb> {
    let t = text.trim();
    if let Some(hex) = t.strip_prefix('#') {
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok().map(|d| d * 17);
        let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        return match hex.len() {
            3 => Some(Rgb {
                r: digit(0)?,
                g: digit(1)?,
                b: digit(2)?,
            }),
            6 => Some(Rgb {
                r: pair(0)?,
                g: pair(2)?,
                b: pair(4)?,
            }),
            _ => None,
        };
    }
    let inner = t.strip_prefix("rgb(")?.strip_suffix(')')?;
    let mut parts = inner.split(',').map(|p| p.trim().parse::<u8>().ok());
    let rgb = Rgb {
        r: parts.next()??,
        g: parts.next()??,
        b: parts.next()??,
    };
    parts.next().is_none().then_some(rgb)
}

/// Shows the system magnifier and returns the colour clicked, or None when the
/// user pressed Escape.
///
/// `NSColorSampler` needs no Screen Recording permission — the system draws
/// the loupe itself — but it must be started on the main thread, and its
/// answer arrives later on that thread's run loop.
#[cfg(target_os = "macos")]
pub fn pick() -> anyhow::Result<Option<Rgb>> {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSColor, NSColorSampler, NSColorSpace};
    use std::cell::RefCell;
    use std::sync::mpsc;
    use std::time::Duration;

    thread_local! {
        // The sampler dies with its last reference, which cancels the pick.
        // Held on the main thread (the only one that touches it) until it
        // answers.
        static ACTIVE: RefCell<Option<Retained<NSColorSampler>>> = const { RefCell::new(None) };
    }

    let (tx, rx) = mpsc::channel::<Option<Rgb>>();
    aloud_paste::on_main_thread(move || {
        let sampler = NSColorSampler::new();
        let block = RcBlock::new(move |color: *mut NSColor| {
            let rgb = unsafe { color.as_ref() }.and_then(|color| {
                let srgb = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
                let byte = |c: f64| (c * 255.0).round().clamp(0.0, 255.0) as u8;
                Some(Rgb {
                    r: byte(srgb.redComponent()),
                    g: byte(srgb.greenComponent()),
                    b: byte(srgb.blueComponent()),
                })
            });
            ACTIVE.with(|slot| slot.borrow_mut().take());
            let _ = tx.send(rgb);
        });
        // SAFETY: the block is called at most once, on this thread.
        unsafe { sampler.showSamplerWithSelectionHandler(&block) };
        ACTIVE.with(|slot| *slot.borrow_mut() = Some(sampler));
    })?;
    // A person decides when to click; a minute is "they walked away".
    Ok(rx.recv_timeout(Duration::from_secs(60)).unwrap_or(None))
}

#[cfg(not(target_os = "macos"))]
pub fn pick() -> anyhow::Result<Option<Rgb>> {
    anyhow::bail!("the colour picker is macOS-only for now")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_forms_it_offers_and_nothing_looser() {
        let orange = Rgb {
            r: 255,
            g: 136,
            b: 0,
        };
        assert_eq!(parse("#ff8800"), Some(orange));
        assert_eq!(parse(" #F80 "), Some(orange));
        assert_eq!(parse("rgb(255, 136, 0)"), Some(orange));
        assert_eq!(parse("rgb(255,136,0)"), Some(orange));
        assert_eq!(parse("rgb(256, 0, 0)"), None);
        assert_eq!(parse("rgb(1, 2)"), None);
        assert_eq!(parse("rgb(1, 2, 3, 4)"), None);
        assert_eq!(parse("#ff88"), None);
        assert_eq!(parse("#gg0000"), None);
        assert_eq!(parse("the colour #ff8800"), None);
    }

    #[test]
    fn writes_what_it_reads() {
        let c = Rgb {
            r: 1,
            g: 171,
            b: 255,
        };
        assert_eq!(c.hex(), "#01abff");
        assert_eq!(parse(&c.hex()), Some(c));
        assert_eq!(parse(&c.css()), Some(c));
    }
}
