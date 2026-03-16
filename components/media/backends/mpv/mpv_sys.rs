/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */


use libc::{c_char, c_double, c_int, c_void};

#[allow(non_camel_case_types)]
pub type mpv_handle = c_void;

#[allow(non_camel_case_types)]
pub type mpv_render_context = c_void;

#[repr(C)]
#[allow(non_camel_case_types, dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum mpv_event_id {
    MPV_EVENT_NONE = 0,
    MPV_EVENT_SHUTDOWN = 1,
    MPV_EVENT_LOG_MESSAGE = 2,
    MPV_EVENT_GET_PROPERTY_REPLY = 3,
    MPV_EVENT_SET_PROPERTY_REPLY = 4,
    MPV_EVENT_COMMAND_REPLY = 5,
    MPV_EVENT_START_FILE = 6,
    MPV_EVENT_END_FILE = 7,
    MPV_EVENT_FILE_LOADED = 8,
    MPV_EVENT_IDLE = 11,
    MPV_EVENT_TICK = 14,
    MPV_EVENT_CLIENT_MESSAGE = 16,
    MPV_EVENT_VIDEO_RECONFIG = 17,
    MPV_EVENT_AUDIO_RECONFIG = 18,
    MPV_EVENT_SEEK = 20,
    MPV_EVENT_PLAYBACK_RESTART = 21,
    MPV_EVENT_PROPERTY_CHANGE = 22,
    MPV_EVENT_QUEUE_OVERFLOW = 24,
    MPV_EVENT_HOOK = 25,
}

#[repr(C)]
pub struct mpv_event_end_file {
    pub reason: c_int,
    pub error: c_int,
}

#[repr(C)]
pub struct mpv_event_property {
    pub name: *const c_char,
    pub format: c_int,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct mpv_event {
    pub event_id: mpv_event_id,
    pub error: c_int,
    pub reply_userdata: u64,
    pub data: *mut c_void,
}

pub const MPV_RENDER_PARAM_INVALID: i32 = 0;
pub const MPV_RENDER_PARAM_API_TYPE: i32 = 1;
pub const MPV_RENDER_PARAM_SW_SIZE: i32 = 17;
pub const MPV_RENDER_PARAM_SW_FORMAT: i32 = 18;
pub const MPV_RENDER_PARAM_SW_STRIDE: i32 = 19;
pub const MPV_RENDER_PARAM_SW_POINTER: i32 = 20;

pub const MPV_RENDER_UPDATE_FRAME: u64 = 1;

#[repr(C)]
pub struct mpv_render_param {
    pub type_: i32,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct MpvStreamCbInfo {
    pub cookie: *mut c_void,
    pub read_fn: Option<unsafe extern "C" fn(*mut c_void, *mut c_char, u64) -> i64>,
    pub seek_fn: Option<unsafe extern "C" fn(*mut c_void, i64) -> i64>,
    pub size_fn: Option<unsafe extern "C" fn(*mut c_void) -> i64>,
    pub close_fn: Option<unsafe extern "C" fn(*mut c_void)>,
    pub cancel_fn: Option<unsafe extern "C" fn(*mut c_void)>,
}

unsafe extern "C" {
    pub fn mpv_create() -> *mut mpv_handle;
    pub fn mpv_initialize(ctx: *mut mpv_handle) -> c_int;
    pub fn mpv_destroy(ctx: *mut mpv_handle);
    pub fn mpv_terminate_destroy(ctx: *mut mpv_handle);

    pub fn mpv_set_property_string(
        ctx: *mut mpv_handle,
        name: *const c_char,
        value: *const c_char,
    ) -> c_int;

    pub fn mpv_get_property_string(ctx: *mut mpv_handle, name: *const c_char) -> *mut c_char;

    pub fn mpv_free(data: *mut c_void);

    pub fn mpv_wait_event(ctx: *mut mpv_handle, timeout: c_double) -> *mut mpv_event;

    pub fn mpv_command(ctx: *mut mpv_handle, args: *const *const c_char) -> c_int;

    pub fn mpv_stream_cb_add_ro(
        ctx: *mut mpv_handle,
        protocol: *const c_char,
        user_data: *mut c_void,
        open_fn: unsafe extern "C" fn(*mut c_void, *mut c_char, *mut MpvStreamCbInfo) -> c_int,
    ) -> c_int;

    pub fn mpv_render_context_create(
        res: *mut *mut mpv_render_context,
        mpv: *mut mpv_handle,
        params: *const mpv_render_param,
    ) -> c_int;

    pub fn mpv_render_context_render(
        ctx: *mut mpv_render_context,
        params: *const mpv_render_param,
    ) -> c_int;

    pub fn mpv_render_context_free(ctx: *mut mpv_render_context);

    pub fn mpv_render_context_set_update_callback(
        ctx: *mut mpv_render_context,
        callback: extern "C" fn(*mut c_void),
        callback_ctx: *mut c_void,
    );

    pub fn mpv_render_context_update(ctx: *mut mpv_render_context) -> u64;
}

use std::ffi::{CStr, CString};

pub struct Mpv {
    pub handle: *mut mpv_handle,
}

impl Mpv {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let handle = mpv_create();
            if handle.is_null() {
                return Err("mpv_create failed".to_string());
            }

            if mpv_initialize(handle) < 0 {
                mpv_destroy(handle);
                return Err("mpv_initialize failed".to_string());
            }

            Ok(Mpv { handle })
        }
    }

    pub fn set_property<T: MpvProperty>(&self, name: &str, value: T) -> Result<(), String> {
        value.set(self.handle, name)
    }

    pub fn get_property_string(&self, name: &str) -> Option<String> {
        unsafe {
            let name_c = CString::new(name).ok()?;
            let result = mpv_get_property_string(self.handle, name_c.as_ptr());
            if result.is_null() {
                return None;
            }
            let cstr = CStr::from_ptr(result);
            let string = cstr.to_string_lossy().into_owned();
            mpv_free(result as *mut c_void);
            Some(string)
        }
    }

    pub fn get_property<T: std::str::FromStr>(&self, name: &str) -> Result<T, String> {
        self.get_property_string(name)
            .ok_or_else(|| format!("Property {} not found", name))?
            .parse()
            .map_err(|_| format!("Failed to parse property {}", name))
    }

    pub fn wait_event(&self, timeout: f64) -> Option<MpvEvent> {
        unsafe {
            let event = mpv_wait_event(self.handle, timeout);
            if event.is_null() {
                return None;
            }
            Some(MpvEvent::from_raw(&*event))
        }
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        unsafe {
            mpv_terminate_destroy(self.handle);
        }
    }
}

pub trait MpvProperty {
    fn set(self, handle: *mut mpv_handle, name: &str) -> Result<(), String>;
}

impl MpvProperty for &str {
    fn set(self, handle: *mut mpv_handle, name: &str) -> Result<(), String> {
        unsafe {
            let name_c = CString::new(name).unwrap();
            let value_c = CString::new(self).unwrap();
            if mpv_set_property_string(handle, name_c.as_ptr(), value_c.as_ptr()) < 0 {
                return Err(format!("Failed to set property {}", name));
            }
            Ok(())
        }
    }
}

impl MpvProperty for bool {
    fn set(self, handle: *mut mpv_handle, name: &str) -> Result<(), String> {
        self.to_string().as_str().set(handle, name)
    }
}

impl MpvProperty for i64 {
    fn set(self, handle: *mut mpv_handle, name: &str) -> Result<(), String> {
        self.to_string().as_str().set(handle, name)
    }
}

impl MpvProperty for f64 {
    fn set(self, handle: *mut mpv_handle, name: &str) -> Result<(), String> {
        self.to_string().as_str().set(handle, name)
    }
}

#[derive(Debug, Clone)]
pub enum MpvEvent {
    None,
    Shutdown,
    StartFile,
    EndFile(i32),
    FileLoaded,
    Idle,
    VideoReconfig,
    AudioReconfig,
    Seek,
    PlaybackRestart,
    #[allow(dead_code)]
    PropertyChange(String),
    Other,
}

impl MpvEvent {
    unsafe fn from_raw(event: &mpv_event) -> Self {
        match event.event_id {
            mpv_event_id::MPV_EVENT_NONE => MpvEvent::None,
            mpv_event_id::MPV_EVENT_SHUTDOWN => MpvEvent::Shutdown,
            mpv_event_id::MPV_EVENT_START_FILE => MpvEvent::StartFile,
            mpv_event_id::MPV_EVENT_END_FILE => {
                if !event.data.is_null() {
                    let data = unsafe { &*(event.data as *const mpv_event_end_file) };
                    MpvEvent::EndFile(data.reason)
                } else {
                    MpvEvent::EndFile(0)
                }
            },
            mpv_event_id::MPV_EVENT_FILE_LOADED => MpvEvent::FileLoaded,
            mpv_event_id::MPV_EVENT_IDLE => MpvEvent::Idle,
            mpv_event_id::MPV_EVENT_VIDEO_RECONFIG => MpvEvent::VideoReconfig,
            mpv_event_id::MPV_EVENT_AUDIO_RECONFIG => MpvEvent::AudioReconfig,
            mpv_event_id::MPV_EVENT_SEEK => MpvEvent::Seek,
            mpv_event_id::MPV_EVENT_PLAYBACK_RESTART => MpvEvent::PlaybackRestart,
            mpv_event_id::MPV_EVENT_PROPERTY_CHANGE => {
                if !event.data.is_null() {
                    let data = unsafe { &*(event.data as *const mpv_event_property) };
                    let name = if !data.name.is_null() {
                        unsafe { CStr::from_ptr(data.name).to_string_lossy().into_owned() }
                    } else {
                        String::new()
                    };
                    MpvEvent::PropertyChange(name)
                } else {
                    MpvEvent::PropertyChange(String::new())
                }
            },
            _ => MpvEvent::Other,
        }
    }
}
