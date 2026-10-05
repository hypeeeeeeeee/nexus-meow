use nexus::{
    imgui::{Slider, Ui},
    paths::get_addon_dir,
};
use std::{fs, path::PathBuf, sync::Mutex};

#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub volume: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { volume: 100 }
    }
}

static SETTINGS: Mutex<Settings> = Mutex::new(Settings { volume: 100 });

impl Settings {
    pub fn get() -> Self {
        *SETTINGS.lock().unwrap()
    }

    fn path() -> Option<PathBuf> {
        get_addon_dir("meow").ok().map(|dir| dir.join("meow.conf"))
    }

    pub fn load() {
        let Some(path) = Self::path() else { return };
        let Ok(contents) = fs::read_to_string(&path) else { return };

        let mut settings = Settings::default();
        for line in contents.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            if key.trim() == "volume" {
                match value.trim().parse::<u32>() {
                    Ok(volume) => settings.volume = volume.min(100),
                    Err(e) => log::warn!("Invalid volume in settings: {e}"),
                }
            }
        }
        *SETTINGS.lock().unwrap() = settings;
    }

    pub fn store() {
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let settings = Self::get();
        if let Err(e) = fs::write(&path, format!("volume={}\n", settings.volume)) {
            log::error!("Could not store settings: {e}");
        }
    }

    pub fn render(ui: &Ui) {
        {
            let mut settings = SETTINGS.lock().unwrap();
            Slider::new("Meow volume", 0u32, 100)
                .display_format("%d%%")
                .build(ui, &mut settings.volume);
        }
        if ui.is_item_deactivated_after_edit() {
            Self::store();
        }
        if ui.button("Test meow") {
            crate::play_meow();
        }
    }
}
