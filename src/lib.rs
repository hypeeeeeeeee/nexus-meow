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
    Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_MEMORY, SND_NODEFAULT},
    core::HSTRING,
};

const MEOW_INTERVAL: Duration = Duration::from_secs(60);
const CHECK_INTERVAL: Duration = Duration::from_secs(1);

static MEOW_WAV: &[u8] = include_bytes!("../meow.wav");

struct State {
    last_meow: Instant,
    last_check: Instant,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn load() {
    let now = Instant::now();
    *STATE.lock().unwrap() = Some(State {
        last_meow: now,
        last_check: now,
    });
    register_render(RenderType::Render, render!(on_frame)).revert_on_unload();
    log::info!("Meow addon loaded");

    play_meow();
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

fn play_meow() {
    if let Some(custom) = get_addon_dir("meow")
        .ok()
        .map(|dir| dir.join("meow.wav"))
        .filter(|path| path.is_file())
    {
        let path = HSTRING::from(custom.as_os_str());
        unsafe {
            let _ = PlaySoundW(&path, None, SND_FILENAME | SND_ASYNC | SND_NODEFAULT);
        }
        return;
    }

    unsafe {
        let _ = PlaySoundW(
            windows::core::PCWSTR(MEOW_WAV.as_ptr() as *const u16),
            None,
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

nexus::export! {
    name: "Meow",
    signature: -0x4D454F57,
    load,
    provider: UpdateProvider::GitHub,
    update_link: "https://github.com/hypeeeeeeeee/nexus-meow"
}
