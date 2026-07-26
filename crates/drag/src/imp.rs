//! macOS drag-out via `NSDraggingSession`. Main-thread only.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{declare_class, msg_send, msg_send_id, mutability, ClassType, DeclaredClass};
use objc2_foundation::MainThreadMarker;
use objc2_app_kit::{
    NSDraggingItem, NSDraggingSession, NSDraggingSource, NSEvent, NSEventMask, NSEventType, NSView,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString, NSURL};

// Our `NSDraggingSource`. It carries no state — the source operation is a
// constant (copy) — but `beginDraggingSession` requires a live source object
// that answers `sourceOperationMaskForDraggingContext`.
declare_class!(
    struct DragSource;

    unsafe impl ClassType for DragSource {
        type Super = NSObject;
        // NSDraggingSource is IsMainThreadOnly, and everything here runs on the
        // main thread anyway (the monitor fires there).
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "AloudDragSource";
    }

    impl DeclaredClass for DragSource {}

    unsafe impl NSObjectProtocol for DragSource {}

    // Conformance so `ProtocolObject::from_ref` can view us as an
    // NSDraggingSource, and so AppKit dispatches the mask method to us.
    unsafe impl NSDraggingSource for DragSource {
        // Outside the app (context 0) and within it, allow Copy. Without this
        // method the drag starts and immediately cancels.
        #[method(draggingSession:sourceOperationMaskForDraggingContext:)]
        unsafe fn source_operation(
            &self,
            _session: &NSDraggingSession,
            _context: isize,
        ) -> usize {
            // NSDragOperationCopy = 1.
            1
        }
    }
);

impl DragSource {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send_id![mtm.alloc::<Self>(), init] }
    }
}

/// The overlay view we begin drags from. Stored so the monitor can use it as the
/// drag source view. A raw pointer kept behind a main-thread-only install.
static mut DRAG_VIEW: *mut NSView = std::ptr::null_mut();
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Installs a local event monitor that turns an armed path + a left-mouse-drag
/// into a system file drag. Idempotent.
pub fn install(ns_view: *mut c_void) {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    // Safety: caller guarantees a live NSView* for the overlay, and install runs
    // on the main thread.
    unsafe {
        DRAG_VIEW = ns_view as *mut NSView;

        let handler = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let event = event.as_ref();
            // Only a left-drag with something armed becomes a drag; everything
            // else passes through untouched so the panel stays interactive.
            if event.r#type() == NSEventType::LeftMouseDragged {
                if let Some(path) = super::take() {
                    begin_drag(&path, event);
                    // Swallow: the drag session now owns the gesture. Returning
                    // the event too would double-handle it.
                    return std::ptr::null_mut();
                }
            }
            // Not ours — hand it back unchanged.
            event as *const NSEvent as *mut NSEvent
        });

        let _monitor = NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseDragged,
            &handler,
        );
        // The monitor is retained by AppKit for the app's life; we deliberately
        // leak our handle to it (the overlay never uninstalls).
        std::mem::forget(_monitor);
    }
}

/// Starts the actual `NSDraggingSession` for `path`, anchored at `event`'s
/// location. Runs on the main thread (the monitor fires there).
unsafe fn begin_drag(path: &str, event: &NSEvent) {
    let view = DRAG_VIEW;
    if view.is_null() {
        return;
    }
    let view: &NSView = &*view;

    // A file URL is NSPasteboardWriting, so it can back a dragging item directly
    // — no NSFilePromiseProvider delegate needed. This is what makes the receiver
    // treat the drop as a real file.
    let ns_path = NSString::from_str(path);
    let url = NSURL::fileURLWithPath(&ns_path);

    let item: Retained<NSDraggingItem> =
        msg_send_id![NSDraggingItem::alloc(), initWithPasteboardWriter: &*url];

    // Frame the drag image at the cursor. A modest square; the OS draws the file
    // icon into it. Location is in window coords, converted to the view.
    let win_loc: NSPoint = event.locationInWindow();
    let local = view.convertPoint_fromView(win_loc, None);
    let size = NSSize::new(64.0, 64.0);
    let frame = NSRect::new(
        NSPoint::new(local.x - size.width / 2.0, local.y - size.height / 2.0),
        size,
    );
    // setDraggingFrame:contents: with a nil image lets the OS supply the file's
    // own icon.
    let _: () = msg_send![&item, setDraggingFrame: frame, contents: std::ptr::null::<AnyObject>()];

    let items = NSArray::from_slice(&[&*item]);
    // The monitor fires on the main thread, so this marker is sound.
    let mtm = MainThreadMarker::new_unchecked();
    let source = DragSource::new(mtm);
    let source_proto: &ProtocolObject<dyn NSDraggingSource> = ProtocolObject::from_ref(&*source);

    // beginDraggingSession retains the source for the session's lifetime, so the
    // local `source` can drop after the call returns — no leak needed.
    let _session: Option<Retained<NSDraggingSession>> = msg_send_id![
        view,
        beginDraggingSessionWithItems: &*items,
        event: event,
        source: source_proto,
    ];
    tracing::debug!(%path, "began file drag session");
}
