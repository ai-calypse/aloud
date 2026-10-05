//! Prints every display and where a top-centred overlay window would sit on
//! each, so a misplaced notch is a paste, not a repro. Reads only.
//! Run: `cargo run -p aloud-screens --example placement`
//!
//! Frames are AppKit points (origin bottom-left of the primary, y up). Every
//! "inside" must read true; a window that is not inside its screen is the
//! straddling bug.

#[cfg(target_os = "macos")]
fn main() {
    use aloud_screens::{current, locate, mouse_location, refresh, top_left_for};

    // The overlay canvas, as declared in tauri.conf.json.
    const WINDOW: (f64, f64) = (1000.0, 720.0);

    refresh();
    let screens = current();
    let mouse = mouse_location();
    let under_mouse = mouse.and_then(|m| locate(&screens, m));
    println!("mouse {mouse:?}  -> screen {under_mouse:?}\n");
    for (i, s) in screens.iter().enumerate() {
        let f = s.frame;
        let (x, top) = top_left_for(f, WINDOW, 0.0);
        let inside =
            x >= f.x && x + WINDOW.0 <= f.max_x() && top <= f.max_y() && top - WINDOW.1 >= f.y;
        println!(
            "[{i}]{}{} \"{}\"  frame x={} y={} w={} h={}  scale {}x  notch {:?}",
            if i == 0 { " primary" } else { "" },
            if under_mouse == Some(i) { " mouse" } else { "" },
            s.name,
            f.x,
            f.y,
            f.w,
            f.h,
            s.scale,
            s.notch
        );
        println!(
            "     window {}x{} -> x={} y={}..{}  inside screen: {inside}",
            WINDOW.0,
            WINDOW.1,
            x,
            top - WINDOW.1,
            top
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("macOS only");
}
