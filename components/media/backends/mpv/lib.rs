/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

extern crate ipc_channel;
extern crate servo_media;
extern crate servo_media_audio;
extern crate servo_media_dummy;
extern crate servo_media_player;
extern crate servo_media_streams;
extern crate servo_media_traits;
extern crate servo_media_webrtc;

mod mpv_sys;
mod player;

use std::sync::{Arc, Mutex};

use ipc_channel::ipc::IpcSender;
use servo_media::{Backend, BackendInit, SupportsMediaType};
use servo_media_audio::context::{AudioContext, AudioContextOptions};
use servo_media_audio::sink::AudioSinkError;
use servo_media_dummy::DummyBackend;
use servo_media_player::audio::AudioRenderer;
use servo_media_player::context::PlayerGLContext;
use servo_media_player::video::VideoFrameRenderer;
use servo_media_player::{Player, PlayerEvent, StreamType};
use servo_media_streams::capture::MediaTrackConstraintSet;
use servo_media_streams::device_monitor::MediaDeviceMonitor;
use servo_media_streams::registry::MediaStreamId;
use servo_media_streams::{MediaOutput, MediaSocket, MediaStreamType};
use servo_media_traits::ClientContextId;
use servo_media_webrtc::{WebRtcController, WebRtcSignaller};

use crate::player::MpvPlayer;

pub struct MpvBackend {
    dummy: DummyBackend,
}

impl BackendInit for MpvBackend {
    fn init() -> Box<dyn Backend> {
        Box::new(MpvBackend {
            dummy: DummyBackend,
        })
    }
}

impl Backend for MpvBackend {
    fn create_player(
        &self,
        id: &ClientContextId,
        stream_type: StreamType,
        sender: IpcSender<PlayerEvent>,
        video_renderer: Option<Arc<Mutex<dyn VideoFrameRenderer>>>,
        _audio_renderer: Option<Arc<Mutex<dyn AudioRenderer>>>,
        _gl_context: Box<dyn PlayerGLContext>,
    ) -> Arc<Mutex<dyn Player>> {
        Arc::new(Mutex::new(MpvPlayer::new(
            id,
            stream_type,
            sender,
            video_renderer,
        )))
    }

    fn create_audiostream(&self) -> MediaStreamId {
        self.dummy.create_audiostream()
    }

    fn create_videostream(&self) -> MediaStreamId {
        self.dummy.create_videostream()
    }

    fn create_stream_output(&self) -> Box<dyn MediaOutput> {
        self.dummy.create_stream_output()
    }

    fn create_stream_and_socket(&self, ty: MediaStreamType) -> (Box<dyn MediaSocket>, MediaStreamId) {
        self.dummy.create_stream_and_socket(ty)
    }

    fn create_audioinput_stream(&self, set: MediaTrackConstraintSet) -> Option<MediaStreamId> {
        self.dummy.create_audioinput_stream(set)
    }

    fn create_videoinput_stream(&self, set: MediaTrackConstraintSet) -> Option<MediaStreamId> {
        self.dummy.create_videoinput_stream(set)
    }

    fn create_audio_context(
        &self,
        id: &ClientContextId,
        options: AudioContextOptions,
    ) -> Result<Arc<Mutex<AudioContext>>, AudioSinkError> {
        self.dummy.create_audio_context(id, options)
    }

    fn create_webrtc(&self, signaller: Box<dyn WebRtcSignaller>) -> WebRtcController {
        self.dummy.create_webrtc(signaller)
    }

    fn can_play_type(&self, media_type: &str) -> SupportsMediaType {
        match media_type {
            "video/mp4" | "video/webm" | "video/ogg" | "video/mpeg" |
            "video/x-matroska" | "video/mkv" | "audio/mpeg" | "audio/mp4" |
            "audio/ogg" | "audio/wav" | "audio/webm" => SupportsMediaType::Probably,
            _ => SupportsMediaType::No,
        }
    }

    fn get_device_monitor(&self) -> Box<dyn MediaDeviceMonitor> {
        self.dummy.get_device_monitor()
    }
}
