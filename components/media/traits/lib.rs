/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Mutex;

use malloc_size_of_derive::MallocSizeOf;
/// An ID for clients to track instances of Players and AudioContexts belonging to the same tab and mute them simultaneously.
/// Current tuple implementation matches one of Servo's BrowsingContextId.
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, MallocSizeOf)]
pub struct ClientContextId(u32, NonZeroU32);

impl ClientContextId {
    pub fn build(a: u32, b: u32) -> ClientContextId {
        ClientContextId(a, NonZeroU32::new(b).unwrap())
    }
}

#[derive(Debug)]
pub struct MediaInstanceError;

impl std::fmt::Display for MediaInstanceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "MediaInstanceError")
    }
}

impl std::error::Error for MediaInstanceError {}

/// Common functionality for all high level media instances
/// These currently are WebAudio AudioContexts and Players.
pub trait MediaInstance: Send {
    fn get_id(&self) -> usize;
    fn mute(&self, val: bool) -> Result<(), MediaInstanceError>;
    fn suspend(&self) -> Result<(), MediaInstanceError>;
    fn resume(&self) -> Result<(), MediaInstanceError>;
}

/// Global audio focus: the player ID that currently owns audio output.
static AUDIO_FOCUS_PLAYER: AtomicI32 = AtomicI32::new(0);

/// Ordered list of player IDs (in creation order = page order).
static PLAYER_REGISTRY: std::sync::LazyLock<Mutex<Vec<i32>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

/// Current index into PLAYER_REGISTRY.
static CURRENT_PAGE: AtomicUsize = AtomicUsize::new(0);

/// Register a player in the ordered registry.
/// The first player registered automatically gets audio focus.
pub fn register_player(player_id: i32) {
    if let Ok(mut reg) = PLAYER_REGISTRY.lock() {
        if !reg.contains(&player_id) {
            let is_first = reg.is_empty();
            reg.push(player_id);
            eprintln!("[AUDIO] registered player-{} (total: {})", player_id, reg.len());
            if is_first {
                AUDIO_FOCUS_PLAYER.store(player_id, Ordering::SeqCst);
                CURRENT_PAGE.store(0, Ordering::SeqCst);
                eprintln!("[AUDIO] player-{} gets initial audio focus", player_id);
            }
        }
    }
}

/// Get the current audio focus player ID.
pub fn audio_focus_player() -> i32 {
    AUDIO_FOCUS_PLAYER.load(Ordering::SeqCst)
}

/// Set the audio focus to a specific player ID.
pub fn set_audio_focus(player_id: i32) {
    AUDIO_FOCUS_PLAYER.store(player_id, Ordering::SeqCst);
    // Update CURRENT_PAGE to match this player's index.
    if let Ok(reg) = PLAYER_REGISTRY.lock() {
        if let Some(idx) = reg.iter().position(|&id| id == player_id) {
            CURRENT_PAGE.store(idx, Ordering::SeqCst);
        }
    }
}

/// Cycle audio focus to the next (+1) or previous (-1) player in page order.
pub fn cycle_audio_focus(delta: i32) {
    if let Ok(reg) = PLAYER_REGISTRY.lock() {
        if reg.is_empty() {
            return;
        }
        let cur = CURRENT_PAGE.load(Ordering::SeqCst);
        let new_idx = if delta > 0 {
            (cur + 1).min(reg.len() - 1)
        } else {
            cur.saturating_sub(1)
        };
        let new_player = reg[new_idx];
        CURRENT_PAGE.store(new_idx, Ordering::SeqCst);
        AUDIO_FOCUS_PLAYER.store(new_player, Ordering::SeqCst);
        eprintln!(
            "[AUDIO] focus cycled: page {} (player-{}) -> page {} (player-{})",
            cur, if cur < reg.len() { reg[cur] } else { -1 }, new_idx, new_player
        );
    }
}

#[derive(MallocSizeOf)]
pub enum BackendMsg {
    /// Message to notify about a media instance shutdown.
    /// The given `usize` is the media instance ID.
    Shutdown {
        context: ClientContextId,
        id: usize,
        tx_ack: Sender<()>,
    },
}
