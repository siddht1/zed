#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::{PlatformDispatcher, TaskLabel};
use async_task::Runnable;
use objc::{
    class, msg_send,
    runtime::{BOOL, YES},
    sel, sel_impl,
};
use parking::{Parker, Unparker};
use parking_lot::Mutex;
use std::{ffi::c_void, sync::Arc, time::Duration};

include!(concat!(env!("OUT_DIR"), "/dispatch_sys.rs"));

pub fn dispatch_get_main_queue() -> dispatch_queue_t {
    unsafe { &_dispatch_main_q as *const _ as dispatch_queue_t }
}

pub struct MacDispatcher {
    parker: Arc<Mutex<Parker>>,
}

impl MacDispatcher {
    pub fn new() -> Self {
        MacDispatcher {
            parker: Arc::new(Mutex::new(Parker::new())),
        }
    }
}

impl PlatformDispatcher for MacDispatcher {
    fn is_main_thread(&self) -> bool {
        let is_main_thread: BOOL = unsafe { msg_send![class!(NSThread), isMainThread] };
        is_main_thread == YES
    }

    fn dispatch(&self, runnable: Runnable, _: Option<TaskLabel>) {
        unsafe {
            dispatch_async_f(
                dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_DEFAULT.try_into().unwrap(), 0),
                runnable.into_raw() as *mut c_void,
                Some(trampoline),
            );
        }
    }

    fn dispatch_on_main_thread(&self, runnable: Runnable) {
        unsafe {
            dispatch_async_f(
                dispatch_get_main_queue(),
                runnable.into_raw() as *mut c_void,
                Some(trampoline),
            );
        }
    }

    fn dispatch_after(&self, duration: Duration, runnable: Runnable) {
        unsafe {
            let queue =
                dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_DEFAULT.try_into().unwrap(), 0);
            let when = dispatch_time(DISPATCH_TIME_NOW as u64, duration.as_nanos() as i64);
            dispatch_after_f(
                when,
                queue,
                runnable.into_raw() as *mut c_void,
                Some(trampoline),
            );
        }
    }

    fn tick(&self, _background_only: bool) -> bool {
        false
    }

    fn park(&self) {
        self.parker.lock().park()
    }

    fn unparker(&self) -> Unparker {
        self.parker.lock().unparker()
    }
}

extern "C" fn trampoline(runnable: *mut c_void) {
    let task = unsafe { Runnable::from_raw(runnable as *mut ()) };
    task.run();
}