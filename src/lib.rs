mod settings;

use nexus::{
    data_link::read_mumble_link,
    gui::{RenderType, register_render},
    imgui::Ui,
    paths::get_addon_dir,
    render,
    UpdateProvider,
};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use windows::{
    Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT},
    core::PCWSTR,
};

use settings::Settings;

const MEOW_INTERVAL: Duration = Duration::from_secs(60);
const CHECK_INTERVAL: Duration = Duration::from_secs(1);

static MEOW_WAV: &[u8] = include_bytes!("../meow.wav");

struct State {
    last_meow: Instant,
    last_check: Instant,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

static PLAYING: Mutex<Vec<u8>> = Mutex::new(Vec::new());

fn load() {
    let now = Instant::now();
    *STATE.lock().unwrap() = Some(State {
        last_meow: now,
        last_check: now,
    });
    Settings::load();
    register_render(RenderType::Render, render!(on_frame)).revert_on_unload();
    register_render(RenderType::OptionsRender, render!(Settings::render)).revert_on_unload();
    log::info!("Meow addon loaded");

    play_meow();
}

fn unload() {
    Settings::store();
    stop_sound();
}

fn in_game() -> bool {
    read_mumble_link()
        .map(|link| link.ui_tick > 0 && link.context.map_id != 0)
        .unwrap_or(false)
}

fn on_frame(_ui: &Ui) {
    let mut guard = STATE.lock().unwrap();
    let Some(state) = guard.as_mut() else { return };

    let now = Instant::now();
    if now.duration_since(state.last_check) < CHECK_INTERVAL {
        return;
    }
    state.last_check = now;

    if !in_game() {
        state.last_meow = now;
        return;
    }

    if now.duration_since(state.last_meow) >= MEOW_INTERVAL {
        state.last_meow = now;
        play_meow();
    }
}

pub(crate) fn play_meow() {
    let volume = Settings::get().volume;
    if volume == 0 {
        return;
    }

    let custom = get_addon_dir("meow")
        .ok()
        .map(|dir| dir.join("meow.wav"))
        .filter(|path| path.is_file())
        .and_then(|path| match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                log::warn!("Could not read custom meow.wav: {e}");
                None
            }
        });
    let mut wav = custom.unwrap_or_else(|| MEOW_WAV.to_vec());

    if volume < 100 && !scale_wav(&mut wav, volume as f32 / 100.0) {
        log::warn!("Unsupported WAV format, playing meow at full volume");
    }

    let mut playing = PLAYING.lock().unwrap();
    stop_sound();
    *playing = wav;
    unsafe {
        let _ = PlaySoundW(
            PCWSTR(playing.as_ptr() as *const u16),
            None,
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

fn stop_sound() {
    unsafe {
        let _ = PlaySoundW(PCWSTR::null(), None, Default::default());
    }
}

fn scale_wav(wav: &mut [u8], factor: f32) -> bool {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return false;
    }

    let mut format = None;
    let mut pos = 12;
    while pos + 8 <= wav.len() {
        let size = u32::from_le_bytes(wav[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let start = pos + 8;
        let end = start.saturating_add(size).min(wav.len());

        match &wav[pos..pos + 4] {
            b"fmt " if end - start >= 16 => {
                let fmt = &wav[start..end];
                let mut tag = u16::from_le_bytes([fmt[0], fmt[1]]);
                let bits = u16::from_le_bytes([fmt[14], fmt[15]]);
                if tag == 0xFFFE && fmt.len() >= 26 {
                    tag = u16::from_le_bytes([fmt[24], fmt[25]]);
                }
                format = Some((tag, bits));
            }
            b"data" => {
                let Some((tag, bits)) = format else { return false };
                return scale_samples(&mut wav[start..end], tag, bits, factor);
            }
            _ => {}
        }

        pos = start.saturating_add(size).saturating_add(size & 1);
    }
    false
}

fn scale_samples(data: &mut [u8], tag: u16, bits: u16, factor: f32) -> bool {
    const PCM: u16 = 1;
    const FLOAT: u16 = 3;

    match (tag, bits) {
        (PCM, 8) => {
            for sample in data {
                *sample = ((*sample as f32 - 128.0) * factor + 128.0).round() as u8;
            }
        }
        (PCM, 16) => {
            for chunk in data.chunks_exact_mut(2) {
                let value = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 * factor;
                chunk.copy_from_slice(&(value.round() as i16).to_le_bytes());
            }
        }
        (PCM, 24) => {
            for chunk in data.chunks_exact_mut(3) {
                let value = i32::from_le_bytes([0, chunk[0], chunk[1], chunk[2]]) >> 8;
                let scaled = ((value as f32 * factor).round() as i32).to_le_bytes();
                chunk.copy_from_slice(&scaled[..3]);
            }
        }
        (PCM, 32) => {
            for chunk in data.chunks_exact_mut(4) {
                let value = i32::from_le_bytes(chunk.try_into().unwrap()) as f64 * factor as f64;
                chunk.copy_from_slice(&(value.round() as i32).to_le_bytes());
            }
        }
        (FLOAT, 32) => {
            for chunk in data.chunks_exact_mut(4) {
                let value = f32::from_le_bytes(chunk.try_into().unwrap()) * factor;
                chunk.copy_from_slice(&value.to_le_bytes());
            }
        }
        _ => return false,
    }
    true
}

nexus::export! {
    name: "Meow",
    signature: -0x4D454F57,
    load,
    unload,
    provider: UpdateProvider::GitHub,
    update_link: "https://github.com/hypeeeeeeeee/nexus-meow"
}
