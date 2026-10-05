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
    collections::hash_map::RandomState,
    hash::BuildHasher,
    ops::Range,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use windows::{
    Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT},
    core::PCWSTR,
};

use settings::Settings;

const CHECK_INTERVAL: Duration = Duration::from_millis(100);
const STALE_TIMEOUT: Duration = Duration::from_secs(2);

static MEOW_WAVS: [&[u8]; 9] = [
    include_bytes!("../meow0.wav"),
    include_bytes!("../meow1.wav"),
    include_bytes!("../meow2.wav"),
    include_bytes!("../meow3.wav"),
    include_bytes!("../meow4.wav"),
    include_bytes!("../meow5.wav"),
    include_bytes!("../meow6.wav"),
    include_bytes!("../meow7.wav"),
    include_bytes!("../meow8.wav"),
];

static LAST_MEOW: AtomicUsize = AtomicUsize::new(usize::MAX);

struct State {
    last_meow: Instant,
    last_check: Instant,
    in_game: bool,
    last_tick: u32,
    tick_changed: Instant,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

struct Playing {
    wav: Vec<u8>,
    until: Option<Instant>,
}

static PLAYING: Mutex<Playing> = Mutex::new(Playing {
    wav: Vec::new(),
    until: None,
});

fn is_playing(now: Instant) -> bool {
    PLAYING
        .lock()
        .unwrap()
        .until
        .is_some_and(|until| now < until)
}

fn load() {
    let now = Instant::now();
    *STATE.lock().unwrap() = Some(State {
        last_meow: now,
        last_check: now,
        in_game: false,
        last_tick: 0,
        tick_changed: now,
    });
    Settings::load();
    register_render(RenderType::Render, render!(on_frame)).revert_on_unload();
    register_render(RenderType::OptionsRender, render!(Settings::render)).revert_on_unload();
    log::info!("Meow addon loaded");
}

fn unload() {
    Settings::store();
    stop_sound();
}

impl State {
    fn update_in_game(&mut self, now: Instant) -> bool {
        let Some(link) = read_mumble_link() else { return true };

        if link.ui_tick != self.last_tick {
            self.last_tick = link.ui_tick;
            self.tick_changed = now;
        }
        let live = now.duration_since(self.tick_changed) < STALE_TIMEOUT;

        link.ui_tick > 0 && link.context.map_id != 0 && live
    }
}

fn on_frame(_ui: &Ui) {
    let mut guard = STATE.lock().unwrap();
    let Some(state) = guard.as_mut() else { return };

    let now = Instant::now();
    if now.duration_since(state.last_check) < CHECK_INTERVAL {
        return;
    }
    state.last_check = now;

    let in_game = state.update_in_game(now);
    let entered = in_game && !state.in_game;
    if in_game != state.in_game {
        state.in_game = in_game;
        log::debug!("In game: {in_game}");
    }

    if !in_game {
        state.last_meow = now;
        return;
    }

    let elapsed = now.duration_since(state.last_meow) >= Settings::get().interval();
    if entered || (elapsed && !is_playing(now)) {
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
    let mut wav = custom.unwrap_or_else(|| random_meow().to_vec());

    if volume < 100 && !scale_wav(&mut wav, volume as f32 / 100.0) {
        log::warn!("Unsupported WAV format, playing meow at full volume");
    }

    let mut playing = PLAYING.lock().unwrap();
    stop_sound();
    playing.until = wav_duration(&wav).map(|duration| Instant::now() + duration);
    playing.wav = wav;
    unsafe {
        let _ = PlaySoundW(
            PCWSTR(playing.wav.as_ptr() as *const u16),
            None,
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

fn random_meow() -> &'static [u8] {
    let random = RandomState::new().hash_one(Instant::now()) as usize;
    let last = LAST_MEOW.load(Ordering::Relaxed);
    let index = if last < MEOW_WAVS.len() {
        let index = random % (MEOW_WAVS.len() - 1);
        if index >= last { index + 1 } else { index }
    } else {
        random % MEOW_WAVS.len()
    };
    LAST_MEOW.store(index, Ordering::Relaxed);
    MEOW_WAVS[index]
}

fn stop_sound() {
    unsafe {
        let _ = PlaySoundW(PCWSTR::null(), None, Default::default());
    }
}

struct WavInfo {
    tag: u16,
    bits: u16,
    byte_rate: u32,
    data: Range<usize>,
}

fn parse_wav(wav: &[u8]) -> Option<WavInfo> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
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
                let byte_rate = u32::from_le_bytes(fmt[8..12].try_into().unwrap());
                let bits = u16::from_le_bytes([fmt[14], fmt[15]]);
                if tag == 0xFFFE && fmt.len() >= 26 {
                    tag = u16::from_le_bytes([fmt[24], fmt[25]]);
                }
                format = Some((tag, bits, byte_rate));
            }
            b"data" => {
                let (tag, bits, byte_rate) = format?;
                return Some(WavInfo {
                    tag,
                    bits,
                    byte_rate,
                    data: start..end,
                });
            }
            _ => {}
        }

        pos = start.saturating_add(size).saturating_add(size & 1);
    }
    None
}

fn wav_duration(wav: &[u8]) -> Option<Duration> {
    let info = parse_wav(wav)?;
    (info.byte_rate > 0)
        .then(|| Duration::from_secs_f64(info.data.len() as f64 / info.byte_rate as f64))
}

fn scale_wav(wav: &mut [u8], factor: f32) -> bool {
    let Some(info) = parse_wav(wav) else { return false };
    scale_samples(&mut wav[info.data], info.tag, info.bits, factor)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_meows_have_duration() {
        for wav in MEOW_WAVS {
            let duration = wav_duration(wav).expect("valid wav");
            assert!(duration > Duration::from_millis(500) && duration < Duration::from_secs(5));
        }
    }

    #[test]
    fn scaling_keeps_wav_valid() {
        let mut wav = MEOW_WAVS[0].to_vec();
        assert!(scale_wav(&mut wav, 0.5));
        assert_eq!(wav_duration(&wav), wav_duration(MEOW_WAVS[0]));
    }
}
