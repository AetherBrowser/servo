/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::ffi::CString;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ipc_channel::ipc::IpcSender;
use libc::{c_char, c_int, c_void, size_t};
use servo_media_player::metadata::Metadata;
use servo_media_player::video::{Buffer, VideoFrame, VideoFrameData, VideoFrameRenderer};
use servo_media_player::{Player, PlayerError, PlayerEvent, PlaybackState, StreamType};
use servo_media_streams::registry::MediaStreamId;
use servo_media_traits::{ClientContextId, MediaInstance, MediaInstanceError};

use crate::mpv_sys::*;

struct StreamReader {
    sender:   IpcSender<PlayerEvent>,
    rx:       Receiver<Vec<u8>>,
    all_data: Vec<u8>,
    read_pos: usize,
    eos:      bool,
    cancelled: Arc<AtomicBool>,
}

fn fill_to(reader: &mut StreamReader, target: usize) {
    let initial_len = reader.all_data.len();
    if initial_len < target {
        eprintln!("[MPV-DBG] fill_to: need {} bytes, have {}, requesting more", target, initial_len);
    }
    while reader.all_data.len() < target && !reader.eos {
        if reader.cancelled.load(Ordering::Relaxed) {
            return;
        }
        match reader.rx.try_recv() {
            Ok(data) => {
                reader.all_data.extend_from_slice(&data);
            },
            Err(mpsc::TryRecvError::Empty) => {
                let _ = reader.sender.send(PlayerEvent::NeedData);
                loop {
                    match reader.rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(data) => {
                            reader.all_data.extend_from_slice(&data);
                            break;
                        },
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if reader.cancelled.load(Ordering::Relaxed) {
                                return;
                            }
                        },
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            reader.eos = true;
                            break;
                        },
                    }
                }
            },
            Err(mpsc::TryRecvError::Disconnected) => {
                reader.eos = true;
            },
        }
    }
}

unsafe extern "C" fn stream_open_cb(
    user_data: *mut c_void,
    uri: *mut c_char,
    info: *mut MpvStreamCbInfo,
) -> c_int {
    let uri_str = if uri.is_null() { "<null>".to_string() } else {
        unsafe { std::ffi::CStr::from_ptr(uri).to_string_lossy().into_owned() }
    };
    eprintln!("[MPV-DBG] stream_open_cb: uri={}", uri_str);
    unsafe {
        (*info).cookie    = user_data;
        (*info).read_fn   = Some(stream_read_cb);
        (*info).seek_fn   = Some(stream_seek_cb);
        (*info).size_fn   = Some(stream_size_cb);
        (*info).close_fn  = Some(stream_close_cb);
        (*info).cancel_fn = Some(stream_cancel_cb);
    }
    0
}

unsafe extern "C" fn stream_read_cb(cookie: *mut c_void, buf: *mut c_char, nbytes: u64) -> i64 {
    let reader = unsafe { &mut *(cookie as *mut StreamReader) };
    if reader.cancelled.load(Ordering::Relaxed) {
        return -1;
    }
    let want   = nbytes as usize;

    fill_to(reader, reader.read_pos + want);

    if reader.cancelled.load(Ordering::Relaxed) {
        return -1;
    }

    let avail = reader.all_data.len()
        .saturating_sub(reader.read_pos)
        .min(want);
    if avail == 0 {
        return 0;
    }

    let src = &reader.all_data[reader.read_pos .. reader.read_pos + avail];
    let dst = unsafe { std::slice::from_raw_parts_mut(buf as *mut u8, avail) };
    dst.copy_from_slice(src);
    reader.read_pos += avail;
    avail as i64
}

unsafe extern "C" fn stream_seek_cb(cookie: *mut c_void, offset: i64) -> i64 {
    let reader = unsafe { &mut *(cookie as *mut StreamReader) };
    if reader.cancelled.load(Ordering::Relaxed) {
        return -1;
    }
    let target = offset as usize;

    fill_to(reader, target);

    if reader.cancelled.load(Ordering::Relaxed) {
        return -1;
    }

    reader.read_pos = target.min(reader.all_data.len());
    reader.read_pos as i64
}

unsafe extern "C" fn stream_size_cb(cookie: *mut c_void) -> i64 {
    let reader = unsafe { &*(cookie as *const StreamReader) };
    let size = if reader.eos { reader.all_data.len() as i64 } else { -1 };
    size
}

unsafe extern "C" fn stream_close_cb(_cookie: *mut c_void) {
    // StreamReader lifetime is managed by Rust (dropped at end of run_player).
}

unsafe extern "C" fn stream_cancel_cb(cookie: *mut c_void) {
    unsafe { &*(cookie as *const StreamReader) }.cancelled.store(true, Ordering::SeqCst);
}

struct PixelBuffer(Arc<Vec<u8>>);

impl Buffer for PixelBuffer {
    fn to_vec(&self) -> Option<VideoFrameData> {
        Some(VideoFrameData::Raw(self.0.clone()))
    }
}

struct SharedState {
    playing:     AtomicBool,
    muted:       AtomicBool,
    volume_bits: AtomicU64,
    rate_bits:   AtomicU64,
    seek_bits:   AtomicU64,  // target seek position in seconds (f64 bits), NaN = no seek pending
    stop:        AtomicBool,
    #[allow(dead_code)]
    id:          usize,
    stream_tx:   Mutex<Option<Sender<Vec<u8>>>>,
    cancelled:   Arc<AtomicBool>,
}

impl SharedState {
    fn volume(&self) -> f64 { f64::from_bits(self.volume_bits.load(Ordering::Relaxed)) }
    fn rate(&self)   -> f64 { f64::from_bits(self.rate_bits.load(Ordering::Relaxed)) }
}

pub struct MpvPlayer {
    id:    usize,
    state: Arc<SharedState>,
}

static PLAYER_ID: AtomicI32 = AtomicI32::new(1);



impl MpvPlayer {
    pub fn new(
        _context_id:    &ClientContextId,
        _stream_type:   StreamType,
        sender:         IpcSender<PlayerEvent>,
        video_renderer: Option<Arc<Mutex<dyn VideoFrameRenderer>>>,
    ) -> Self {
        let id = PLAYER_ID.fetch_add(1, Ordering::Relaxed) as usize;
        servo_media_traits::register_player(id as i32);

        let (stream_tx, stream_rx) = mpsc::channel::<Vec<u8>>();
        let cancelled = Arc::new(AtomicBool::new(false));

        let state = Arc::new(SharedState {
            playing:     AtomicBool::new(true), // MPV starts with pause="no"
            muted:       AtomicBool::new(false),
            volume_bits: AtomicU64::new(f64::to_bits(1.0)),
            rate_bits:   AtomicU64::new(f64::to_bits(1.0)),
            seek_bits:   AtomicU64::new(f64::to_bits(f64::NAN)),
            stop:        AtomicBool::new(false),
            id,
            stream_tx:   Mutex::new(Some(stream_tx)),
            cancelled:   cancelled.clone(),
        });

        {
            let state = state.clone();
            thread::Builder::new()
                .name(format!("mpv-player-{}", id))
                .spawn(move || run_player(id, stream_rx, cancelled, state, sender, video_renderer))
                .expect("Failed to spawn mpv player thread");
        }
        MpvPlayer { id, state }
    }
}

impl Drop for MpvPlayer {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::SeqCst);
        self.state.cancelled.store(true, Ordering::SeqCst);
        let _ = self.state.stream_tx.lock().map(|mut g| g.take());
    }
}

impl Player for MpvPlayer {
    fn play(&self) -> Result<(), PlayerError> {
        eprintln!("[MPV-API] player-{} play()", self.id);
        self.state.playing.store(true, Ordering::Relaxed);
        let prev = servo_media_traits::audio_focus_player();
        servo_media_traits::set_audio_focus(self.id as i32);
        Ok(())
    }
    fn pause(&self) -> Result<(), PlayerError> {
        eprintln!("[MPV-API] player-{} pause()", self.id);
        self.state.playing.store(false, Ordering::Relaxed);
        Ok(())
    }
    fn paused(&self) -> bool { !self.state.playing.load(Ordering::Relaxed) }
    fn can_resume(&self) -> bool { true }
    fn stop(&self) -> Result<(), PlayerError> {
        eprintln!("[MPV-API] player-{} stop()", self.id);
        self.state.stop.store(true, Ordering::SeqCst);
        self.state.cancelled.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn seek(&self, time: f64) -> Result<(), PlayerError> {
        eprintln!("[MPV-API] player-{} seek({})", self.id, time);
        self.state.seek_bits.store(f64::to_bits(time), Ordering::SeqCst);
        Ok(())
    }
    fn seekable(&self) -> Vec<Range<f64>> { vec![0.0..f64::MAX] }
    fn set_mute(&self, m: bool) -> Result<(), PlayerError> {
        self.state.muted.store(m, Ordering::Relaxed);
        Ok(())
    }
    fn muted(&self) -> bool { self.state.muted.load(Ordering::Relaxed) }
    fn set_volume(&self, v: f64) -> Result<(), PlayerError> {
        self.state.volume_bits.store(f64::to_bits(v), Ordering::Relaxed);
        Ok(())
    }
    fn volume(&self) -> f64 { self.state.volume() }
    fn set_input_size(&self, _: u64) -> Result<(), PlayerError> { Ok(()) }
    fn set_playback_rate(&self, r: f64) -> Result<(), PlayerError> {
        self.state.rate_bits.store(f64::to_bits(r), Ordering::Relaxed);
        Ok(())
    }
    fn playback_rate(&self) -> f64 { self.state.rate() }

    fn push_data(&self, data: Vec<u8>) -> Result<(), PlayerError> {
        let guard = self.state.stream_tx.lock().unwrap();
        match guard.as_ref() {
            None => {
                Err(PlayerError::EOSFailed)
            },
            Some(tx) => tx.send(data).map_err(|e| {
                PlayerError::BufferPushFailed
            }),
        }
    }

    fn end_of_stream(&self) -> Result<(), PlayerError> {
        let _ = self.state.stream_tx.lock().unwrap().take();
        Ok(())
    }

    fn buffered(&self) -> Vec<Range<f64>> { vec![] }
    fn set_stream(&self, _: &MediaStreamId, _: bool) -> Result<(), PlayerError> {
        Err(PlayerError::SetStreamFailed)
    }
    fn render_use_gl(&self) -> bool { false }
    fn set_audio_track(&self, _: i32, _: bool) -> Result<(), PlayerError> { Ok(()) }
    fn set_video_track(&self, _: i32, _: bool) -> Result<(), PlayerError> { Ok(()) }
}

impl MediaInstance for MpvPlayer {
    fn get_id(&self) -> usize { self.id }
    fn mute(&self, val: bool) -> Result<(), MediaInstanceError> {
        self.state.muted.store(val, Ordering::Relaxed);
        Ok(())
    }
    fn suspend(&self) -> Result<(), MediaInstanceError> {
        self.state.playing.store(false, Ordering::Relaxed);
        Ok(())
    }
    fn resume(&self) -> Result<(), MediaInstanceError> {
        self.state.playing.store(true, Ordering::Relaxed);
        Ok(())
    }
}

fn run_player(
    player_id:      usize,
    stream_rx:      Receiver<Vec<u8>>,
    cancelled:      Arc<AtomicBool>,
    state:          Arc<SharedState>,
    sender:         IpcSender<PlayerEvent>,
    video_renderer: Option<Arc<Mutex<dyn VideoFrameRenderer>>>,
) {

    let mpv = match Mpv::new() {
        Ok(m) => { eprintln!("[MPV-DBG] run_player: Mpv::new() succeeded"); m },
        Err(e) => {
            log::error!("mpv: failed to create: {:?}", e);
            let _ = sender.send(PlayerEvent::Error("mpv_create failed".to_string()));
            return;
        }
    };

    macro_rules! set_prop {
        ($name:expr, $val:expr) => {{
            match mpv.set_property($name, $val) {
                Ok(_) => eprintln!("[MPV-DBG] set_property({}) ok", $name),
                Err(e) => eprintln!("[MPV-DBG] set_property({}) FAILED: {}", $name, e),
            }
        }};
    }

    set_prop!("hwdec", "vaapi"); // Can be "no" or "vaapi" or "auto"
    set_prop!("vo", "libmpv");
    set_prop!("msg-level", "all=v");
    set_prop!("input-default-bindings", false);
    set_prop!("input-vo-keyboard", false);
    set_prop!("osc", false);
    set_prop!("demuxer-max-bytes", "4MiB");
    set_prop!("demuxer-readahead-secs", 5i64);
    set_prop!("cache", "yes");
    set_prop!("cache-secs", 5i64);
    set_prop!("keep-open", "yes");
    set_prop!("pause", "no");
    // Only player 1 gets audio.
    let init_volume = if player_id == 1 { 100i64 } else { 0i64 };
    eprintln!("[MPV-DBG] player-{} init volume={}", player_id, init_volume);
    set_prop!("volume", init_volume);
    set_prop!("vd", "openh264,h264,h264_vaapi");

    let ctx = mpv.handle;

    let mut reader = Box::new(StreamReader {
        sender:    sender.clone(),
        rx:        stream_rx,
        all_data:  Vec::new(),
        read_pos:  0,
        eos:       false,
        cancelled: cancelled.clone(),
    });
    let reader_ptr = reader.as_mut() as *mut StreamReader as *mut c_void;

    {
        let protocol = CString::new("servo").unwrap();
        let rc = unsafe {
            mpv_stream_cb_add_ro(ctx, protocol.as_ptr(), reader_ptr, stream_open_cb)
        };
        if rc != 0 {
            log::error!("mpv: mpv_stream_cb_add_ro failed: {}", rc);
            let _ = sender.send(PlayerEvent::Error("mpv_stream_cb_add_ro failed".to_string()));
            return;
        }
    }

    let api_type = CString::new("sw").unwrap();
    let init_params = [
        mpv_render_param { type_: MPV_RENDER_PARAM_API_TYPE, data: api_type.as_ptr() as *mut c_void },
        mpv_render_param { type_: MPV_RENDER_PARAM_INVALID,  data: std::ptr::null_mut() },
    ];
    let mut render_ctx: *mut mpv_render_context = std::ptr::null_mut();
    let rc_render = unsafe { mpv_render_context_create(&mut render_ctx, ctx, init_params.as_ptr()) };
    if rc_render != 0 {
        log::error!("mpv: mpv_render_context_create failed");
        let _ = sender.send(PlayerEvent::Error("mpv_render_context_create failed".to_string()));
        return;
    }

    struct UpdateCtx { flag: Arc<AtomicBool> }
    extern "C" fn update_cb(user: *mut c_void) {
        unsafe { &*(user as *const UpdateCtx) }.flag.store(true, Ordering::Relaxed);
    }
    let update_flag = Arc::new(AtomicBool::new(false));
    let update_ctx  = Box::new(UpdateCtx { flag: update_flag.clone() });
    unsafe {
        mpv_render_context_set_update_callback(
            render_ctx, update_cb, Box::into_raw(update_ctx) as *mut c_void,
        );
    }

    let url = CString::new(format!("servo://player-{}", player_id)).unwrap();
    {
        let cmd = CString::new("loadfile").unwrap();
        let args: [*const c_char; 3] = [cmd.as_ptr(), url.as_ptr(), std::ptr::null()];
        let rc_load = unsafe { mpv_command(ctx, args.as_ptr()) };
    }

    let mut video_width:  i32   = 0;
    let mut video_height: i32   = 0;
    let mut last_playing        = true; // synced: MPV starts with pause="no"
    let mut last_volume         = 0.0f64;
    let mut last_rate           = 1.0f64;
    let mut last_duration       = -1.0f64;
    let mut metadata_sent       = false;
    let mut eof_sent            = false;
    let mut pending_seek_pos: f64 = f64::NAN; // track requested seek position for SeekDone

    println!("🎬 MPV player loop running for player-{}", player_id);
    loop {
        if state.stop.load(Ordering::Relaxed) { break; }

        let playing = state.playing.load(Ordering::Relaxed);
        if playing != last_playing {
            let pause_val = if playing { "no" } else { "yes" };
            eprintln!("[MPV-LOOP] player-{} pause={} (playing={})", player_id, pause_val, playing);
            let _ = mpv.set_property("pause", pause_val);
            last_playing = playing;
            let ps = if playing { PlaybackState::Playing } else { PlaybackState::Paused };
            let _ = sender.send(PlayerEvent::StateChanged(ps));
        }

        // Audio focus: only the focused player has volume.
        let focus_id = servo_media_traits::audio_focus_player();
        let has_focus = focus_id == player_id as i32;
        let target_vol: i64 = if has_focus { 100 } else { 0 };
        if target_vol != last_volume as i64 {
            let _ = mpv.set_property("volume", target_vol);
            last_volume = target_vol as f64;
        }

        // Handle pending seek.
        let seek_pos = f64::from_bits(state.seek_bits.load(Ordering::SeqCst));
        if !seek_pos.is_nan() {
            state.seek_bits.store(f64::to_bits(f64::NAN), Ordering::SeqCst);
            eprintln!("[MPV-LOOP] player-{} seeking to {}s", player_id, seek_pos);
            pending_seek_pos = seek_pos;
            let _ = mpv.set_property("time-pos", seek_pos);
        }

        let rate = state.rate();
        if (rate - last_rate).abs() > 0.001 {
            let _ = mpv.set_property("speed", rate);
            last_rate = rate;
        }

        while let Some(ev) = mpv.wait_event(0.0) {
            eprintln!("[MPV-DBG] event: {:?}", ev);
            match ev {
                MpvEvent::None => break,
                MpvEvent::Shutdown => {
                    eprintln!("[MPV-DBG] MPV_EVENT_SHUTDOWN — stopping");
                    state.stop.store(true, Ordering::Relaxed);
                    break;
                },
                MpvEvent::StartFile => {
                    eprintln!("[MPV-DBG] MPV_EVENT_START_FILE — file loading started");
                },
                MpvEvent::FileLoaded => {
                    eprintln!("[MPV-DBG] MPV_EVENT_FILE_LOADED — file loaded successfully");
                    let w: Result<f64, _> = mpv.get_property("width");
                    let h: Result<f64, _> = mpv.get_property("height");
                    let dur: Result<f64, _> = mpv.get_property("duration");
                    let _ = sender.send(PlayerEvent::StateChanged(PlaybackState::Paused));
                },
                MpvEvent::EndFile(reason) => {
                    eprintln!("[MPV-DBG] MPV_EVENT_END_FILE reason={}", reason);
                    // reason: 0=eof, 1=stop, 2=quit, 3=error, 4=redirect
                    if reason < 0 {
                        log::error!("mpv: end file with error: {}", reason);
                        let _ = sender.send(PlayerEvent::Error("playback error".to_string()));
                        break;
                    } else if reason == 3 {
                        eprintln!("[MPV-DBG] EndFile: mpv error (reason=3)");
                        let _ = sender.send(PlayerEvent::Error("playback error (end_file reason=3)".to_string()));
                        break;
                    } else {
                        // EOF: send final duration and position so DOM knows playback ended.
                        if let Ok(dur) = mpv.get_property::<f64>("duration") {
                            if dur > 0.0 {
                                let _ = sender.send(PlayerEvent::DurationChanged(Some(
                                    std::time::Duration::from_secs_f64(dur),
                                )));
                                let _ = sender.send(PlayerEvent::PositionChanged(dur));
                            }
                        }
                        // EOF: don't break — keep the loop alive so seek/replay works.
                        let _ = sender.send(PlayerEvent::EndOfStream);
                        let _ = sender.send(PlayerEvent::StateChanged(PlaybackState::Stopped));
                    }
                },
                MpvEvent::VideoReconfig => {
                    eprintln!("[MPV-DBG] MPV_EVENT_VIDEO_RECONFIG");
                    let w = mpv.get_property::<f64>("width");
                    let h = mpv.get_property::<f64>("height");
                    if let (Ok(w), Ok(h)) = (w, h) {
                        if w > 0.0 && h > 0.0 {
                            video_width = w as i32;
                            video_height = h as i32;
                            metadata_sent = false;
                            log::debug!("mpv: video reconfig {}x{}", video_width, video_height);
                        } else {
                            eprintln!("[MPV-DBG]   WARNING: width/height are zero! (w={} h={})", w, h);
                        }
                    } else {
                        eprintln!("[MPV-DBG]   WARNING: could not get width/height properties");
                    }
                },
                MpvEvent::AudioReconfig => {
                    eprintln!("[MPV-DBG] MPV_EVENT_AUDIO_RECONFIG");
                },
                MpvEvent::Seek => {
                    eprintln!("[MPV-DBG] MPV_EVENT_SEEK");
                    let _ = sender.send(PlayerEvent::StateChanged(PlaybackState::Buffering));
                },
                MpvEvent::PlaybackRestart => {
                    eprintln!("[MPV-DBG] MPV_EVENT_PLAYBACK_RESTART for player-{}", player_id);
                    // PlaybackRestart fires after a seek completes — send SeekDone so the DOM
                    // clears its `seeking` flag and resumes normal playback processing.
                    if !pending_seek_pos.is_nan() {
                        let done_pos = pending_seek_pos;
                        pending_seek_pos = f64::NAN;
                        eprintln!("[MPV-DBG]   sending SeekDone({})", done_pos);
                        let _ = sender.send(PlayerEvent::SeekDone(done_pos));
                    }
                    let _ = sender.send(PlayerEvent::StateChanged(PlaybackState::Playing));
                },
                MpvEvent::Idle => {
                    eprintln!("[MPV-DBG] MPV_EVENT_IDLE");
                },
                MpvEvent::PropertyChange(ref name) => {
                    eprintln!("[MPV-DBG] MPV_EVENT_PROPERTY_CHANGE: {}", name);
                },
                MpvEvent::Other => {
                    eprintln!("[MPV-DBG] unknown event (Other)");
                },
            }
        }

        if !metadata_sent && video_width > 0 && video_height > 0 {
            let dur = mpv.get_property::<f64>("duration").ok();
            let _ = sender.send(PlayerEvent::MetadataUpdated(Metadata {
                duration:     dur.filter(|&d| d > 0.0).map(std::time::Duration::from_secs_f64),
                width:        video_width as u32,
                height:       video_height as u32,
                format:       String::from("bgra"),
                is_seekable:  true,
                video_tracks: vec![String::from("video")],
                audio_tracks: vec![String::from("audio")],
                is_live:      false,
                title:        None,
            }));
            metadata_sent = true;
        }

        if let Some(ref renderer) = video_renderer {
            let update_flag_val = update_flag.swap(false, Ordering::Relaxed);
            let render_update_flags = unsafe { mpv_render_context_update(render_ctx) };
            let wants_render = update_flag_val || (render_update_flags & MPV_RENDER_UPDATE_FRAME != 0);

            if wants_render && video_width > 0 && video_height > 0 {
                let byte_count = (video_width * video_height * 4) as usize;
                let mut pixels = vec![0u8; byte_count];
                let stride     = (video_width * 4) as size_t;
                let mut size   = [video_width, video_height];
                let fmt        = CString::new("bgra").unwrap();

                let rp = [
                    mpv_render_param { type_: MPV_RENDER_PARAM_SW_SIZE,    data: size.as_mut_ptr() as *mut c_void },
                    mpv_render_param { type_: MPV_RENDER_PARAM_SW_FORMAT,  data: fmt.as_ptr() as *mut c_void },
                    mpv_render_param { type_: MPV_RENDER_PARAM_SW_STRIDE,  data: &stride as *const size_t as *mut c_void },
                    mpv_render_param { type_: MPV_RENDER_PARAM_SW_POINTER, data: pixels.as_mut_ptr() as *mut c_void },
                    mpv_render_param { type_: MPV_RENDER_PARAM_INVALID,    data: std::ptr::null_mut() },
                ];

                let render_rc = unsafe { mpv_render_context_render(render_ctx, rp.as_ptr()) };
                if render_rc == 0 {
                    let buf = Arc::new(PixelBuffer(Arc::new(pixels)));
                    match VideoFrame::new(video_width, video_height, buf) {
                        Some(frame) => {
                            if let Ok(mut r) = renderer.lock() {
                                eprintln!("[MPV-RENDER] player-{} rendering frame", player_id);
                                r.render(frame);
                            } else {
                                eprintln!("[MPV-DBG] WARNING: could not lock renderer");
                            }
                            let _ = sender.send(PlayerEvent::VideoFrameUpdated);
                        },
                        None => {
                            eprintln!("[MPV-DBG] WARNING: VideoFrame::new returned None for {}x{}", video_width, video_height);
                        },
                    }
                } else {
                    eprintln!("[MPV-DBG] mpv_render_context_render FAILED: rc={}", render_rc);
                }
            } else if wants_render && (video_width == 0 || video_height == 0) {
                eprintln!("[MPV-DBG] wants_render=true but video dimensions are zero ({}x{}), skipping render", video_width, video_height);
            }
        } else {
            static NO_RENDERER_WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !NO_RENDERER_WARNED.swap(true, Ordering::Relaxed) {
                eprintln!("[MPV-DBG] WARNING: video_renderer is None — frames will NOT be rendered!");
            }
        }

        if let Ok(pos) = mpv.get_property::<f64>("time-pos") {
            let _ = sender.send(PlayerEvent::PositionChanged(pos));
        }
        if let Ok(dur) = mpv.get_property::<f64>("duration") {
            if dur > 0.0 && (dur - last_duration).abs() > 0.01 {
                last_duration = dur;
                let _ = sender.send(PlayerEvent::DurationChanged(Some(
                    std::time::Duration::from_secs_f64(dur),
                )));
            }
        }

        // Detect end-of-file with keep-open=yes (MPV pauses at last frame instead of EndFile).
        if let Ok(eof) = mpv.get_property::<bool>("eof-reached") {
            if eof && !eof_sent {
                eof_sent = true;
                eprintln!("[MPV-LOOP] player-{} eof-reached=true, sending EndOfStream", player_id);
                // Send final duration and position so DOM knows playback ended.
                if last_duration > 0.0 {
                    let _ = sender.send(PlayerEvent::PositionChanged(last_duration));
                }
                let _ = sender.send(PlayerEvent::EndOfStream);
                let _ = sender.send(PlayerEvent::StateChanged(PlaybackState::Stopped));
            } else if !eof && eof_sent {
                // Reset when seeking back from EOF.
                eof_sent = false;
                eprintln!("[MPV-LOOP] player-{} eof-reached=false, reset eof_sent", player_id);
            }
        }

        thread::sleep(Duration::from_millis(16));
    }

    state.stop.store(true, Ordering::SeqCst);

    // IMPORTANT: destroy MPV first so it stops calling stream callbacks,
    // then drop the reader. Reverse order = use-after-free.
    unsafe {
        mpv_render_context_free(render_ctx);
    }
    // mpv handle is dropped here when `mpv` goes out of scope.
    drop(mpv);
    drop(reader);
}