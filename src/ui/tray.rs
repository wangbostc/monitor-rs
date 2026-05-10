//! NSStatusItem integration. Creates a system status item, sets its title
//! from the latest sample, and posts a callback when the user clicks it so
//! the main thread can toggle the popover viewport.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{NSStatusBar, NSStatusItem, NSVariableStatusItemLength};
use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSString};

use crate::format::render_menu_bar;
use crate::settings::Settings;
use crate::store::SampleStore;

type ClickCallback = Arc<dyn Fn() + Send + Sync + 'static>;

define_class!(
    // SAFETY:
    // - The superclass NSObject does not have any subclassing requirements.
    // - `ClickTarget` does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MonitorRsClickTarget"]
    #[ivars = ClickCallback]
    struct ClickTarget;

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for ClickTarget {}

    impl ClickTarget {
        // SAFETY: signature matches `- (void)click:(id)sender;`
        #[unsafe(method(click:))]
        fn click(&self, _sender: Option<&AnyObject>) {
            (self.ivars())();
        }
    }
);

impl ClickTarget {
    fn new(callback: ClickCallback, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(callback);
        // SAFETY: NSObject's `init` is safe to call on a freshly allocated object.
        unsafe { msg_send![super(this), init] }
    }
}

pub struct Tray {
    item: Retained<NSStatusItem>,
    // Hold the click target alive — `setTarget:` stores it as a weak reference.
    _target: Retained<ClickTarget>,
}

impl Tray {
    pub fn new(on_click: ClickCallback, mtm: MainThreadMarker) -> Self {
        let status_bar = NSStatusBar::systemStatusBar();
        let item = status_bar.statusItemWithLength(NSVariableStatusItemLength);

        // Initial title so the bar entry is visible before the first sample arrives.
        if let Some(button) = item.button(mtm) {
            button.setTitle(&NSString::from_str("monitor-rs"));
        }

        // Build a target/action pair that calls our Rust closure on click.
        let target = ClickTarget::new(on_click, mtm);
        if let Some(button) = item.button(mtm) {
            // SAFETY:
            // - target outlives the button (we hold it on Tray; setTarget is weak).
            // - the `click:` selector exists on ClickTarget and matches `@selector(click:)`.
            let target_obj: &AnyObject = (*target).as_ref();
            unsafe {
                button.setTarget(Some(target_obj));
                let action: Sel = sel!(click:);
                button.setAction(Some(action));
            }
        }

        Self {
            item,
            _target: target,
        }
    }

    pub fn refresh(&self, store: &SampleStore, settings: &Settings, mtm: MainThreadMarker) {
        let Some(latest) = store.latest() else { return };
        let text = render_menu_bar(&settings.menu_bar_format, latest);
        if let Some(button) = self.item.button(mtm) {
            button.setTitle(&NSString::from_str(&text));
        }
    }
}
