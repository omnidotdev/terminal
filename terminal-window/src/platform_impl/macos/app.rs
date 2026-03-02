#![allow(clippy::unnecessary_cast)]

use objc2::{define_class, msg_send, ClassType};
use objc2_app_kit::{
    NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSResponder,
};
use objc2_foundation::{MainThreadMarker, NSObject};

use super::app_delegate::ApplicationDelegate;
use crate::event::{DeviceEvent, ElementState};

define_class!(
    #[unsafe(super(NSApplication, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "WinitApplication"]
    pub(super) struct WinitApplication;

    impl WinitApplication {
        #[unsafe(method(sendEvent:))]
        fn send_event(&self, event: &NSEvent) {
            let event_type = unsafe { event.r#type() };
            let modifier_flags = unsafe { event.modifierFlags() };
            if event_type == NSEventType::KeyUp
                && modifier_flags.contains(NSEventModifierFlags::Command)
            {
                if let Some(key_window) = self.keyWindow() {
                    key_window.sendEvent(event);
                }
            } else {
                let delegate = ApplicationDelegate::get(MainThreadMarker::from(self));
                maybe_dispatch_device_event(&delegate, event);
                unsafe { msg_send![super(self), sendEvent: event] }
            }
        }
    }
);

fn maybe_dispatch_device_event(delegate: &ApplicationDelegate, event: &NSEvent) {
    let event_type = unsafe { event.r#type() };
    #[allow(non_upper_case_globals)]
    match event_type {
        NSEventType::MouseMoved
        | NSEventType::LeftMouseDragged
        | NSEventType::OtherMouseDragged
        | NSEventType::RightMouseDragged => {
            let delta_x = unsafe { event.deltaX() } as f64;
            let delta_y = unsafe { event.deltaY() } as f64;

            if delta_x != 0.0 {
                delegate.queue_device_event(DeviceEvent::Motion {
                    axis: 0,
                    value: delta_x,
                });
            }

            if delta_y != 0.0 {
                delegate.queue_device_event(DeviceEvent::Motion {
                    axis: 1,
                    value: delta_y,
                })
            }

            if delta_x != 0.0 || delta_y != 0.0 {
                delegate.queue_device_event(DeviceEvent::MouseMotion {
                    delta: (delta_x, delta_y),
                });
            }
        }
        NSEventType::LeftMouseDown
        | NSEventType::RightMouseDown
        | NSEventType::OtherMouseDown => {
            delegate.queue_device_event(DeviceEvent::Button {
                button: unsafe { event.buttonNumber() } as u32,
                state: ElementState::Pressed,
            });
        }
        NSEventType::LeftMouseUp
        | NSEventType::RightMouseUp
        | NSEventType::OtherMouseUp => {
            delegate.queue_device_event(DeviceEvent::Button {
                button: unsafe { event.buttonNumber() } as u32,
                state: ElementState::Released,
            });
        }
        _ => (),
    }
}
