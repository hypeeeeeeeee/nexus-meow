use nexus::{
    imgui::{Slider, Ui},
    paths::get_addon_dir,
};
use std::{fs, path::PathBuf, sync::Mutex, time::Duration};

#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub volume: u32,
    pub interval: u32,
}

impl Settings {
    const DEFAULT: Self = Self {
        volume: 100,
        interval: 60,
    };
    const MIN_INTERVAL: u32 = 1;
    const MAX_INTERVAL: u32 = 60;
}

impl Default for Settings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

static SETTINGS: Mutex<Settings> = Mutex::new(Settings::DEFAULT);

impl Settings {
    pub fn get() -> Self {
        *SETTINGS.lock().unwrap()
    }

    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.interval.into())
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
            let (key, value) = (key.trim(), value.trim());
            match key {
                "volume" => match value.parse::<u32>() {
                    Ok(volume) => settings.volume = volume.min(100),
                    Err(e) => log::warn!("Invalid volume in settings: {e}"),
                },
                "interval" => match value.parse::<u32>() {
                    Ok(interval) => {
                        settings.interval = interval.clamp(Self::MIN_INTERVAL, Self::MAX_INTERVAL)
                    }
                    Err(e) => log::warn!("Invalid interval in settings: {e}"),
                },
                _ => {}
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
        let contents = format!(
            "volume={}\ninterval={}\n",
            settings.volume, settings.interval
        );
        if let Err(e) = fs::write(&path, contents) {
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
        {
            let mut settings = SETTINGS.lock().unwrap();
            Slider::new("Meow interval", Self::MIN_INTERVAL, Self::MAX_INTERVAL)
                .display_format("%d s")
                .build(ui, &mut settings.interval);
        }
        if ui.is_item_deactivated_after_edit() {
            Self::store();
        }
        if ui.button("Test meow") {
            crate::play_meow();
        }
    }
}
