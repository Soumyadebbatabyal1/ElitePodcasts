use adw::prelude::*;

const SCHEMA_ID: &str = "io.github.elite.Podcasts";
pub const SPEEDS: [f64; 6] = [0.75, 1.0, 1.25, 1.5, 1.75, 2.0];
pub const SKIP_LENGTHS: [i32; 4] = [5, 10, 15, 30];

pub struct AppSettings {
    values: gtk::gio::Settings,
}

impl AppSettings {
    pub fn load() -> Result<Self, String> {
        let source = gtk::gio::SettingsSchemaSource::from_directory(
            env!("OUT_DIR"), None, false,
        ).map_err(|error| format!("Could not load application settings: {error}"))?;
        let schema = source.lookup(SCHEMA_ID, false)
            .ok_or("The Elite Podcasts settings schema is missing")?;
        let values = gtk::gio::Settings::new_full(
            &schema, None::<&gtk::gio::SettingsBackend>, None,
        );
        let settings = Self { values };
        settings.apply_appearance();
        settings.values.connect_changed(Some("appearance"), |values, _| {
            apply_appearance(&values.string("appearance"));
        });
        Ok(settings)
    }

    pub fn values(&self) -> &gtk::gio::Settings {
        &self.values
    }

    pub fn appearance(&self) -> String {
        self.values.string("appearance").to_string()
    }

    pub fn set_appearance(&self, value: &str) -> Result<(), String> {
        if !matches!(value, "system" | "light" | "dark") {
            return Err("Unknown appearance setting".to_string());
        }
        self.values.set_string("appearance", value)
            .map_err(|error| format!("Could not save appearance: {error}"))
    }

    pub fn default_speed(&self) -> f64 {
        let saved = self.values.double("default-speed");
        if SPEEDS.contains(&saved) { saved } else { 1.0 }
    }

    pub fn set_default_speed(&self, speed: f64) -> Result<(), String> {
        if !SPEEDS.contains(&speed) {
            return Err("Unknown playback speed".to_string());
        }
        self.values.set_double("default-speed", speed)
            .map_err(|error| format!("Could not save default speed: {error}"))
    }

    pub fn skip_backward_seconds(&self) -> i64 {
        let saved = self.values.int("skip-backward-seconds");
        if SKIP_LENGTHS.contains(&saved) { i64::from(saved) } else { 10 }
    }

    pub fn skip_forward_seconds(&self) -> i64 {
        let saved = self.values.int("skip-forward-seconds");
        if SKIP_LENGTHS.contains(&saved) { i64::from(saved) } else { 10 }
    }

    pub fn set_skip_backward_seconds(&self, seconds: i32) -> Result<(), String> {
        self.set_skip("skip-backward-seconds", seconds)
    }

    pub fn set_skip_forward_seconds(&self, seconds: i32) -> Result<(), String> {
        self.set_skip("skip-forward-seconds", seconds)
    }

    fn set_skip(&self, key: &str, seconds: i32) -> Result<(), String> {
        if !SKIP_LENGTHS.contains(&seconds) {
            return Err("Unknown skip length".to_string());
        }
        self.values.set_int(key, seconds)
            .map_err(|error| format!("Could not save skip length: {error}"))
    }

    fn apply_appearance(&self) {
        apply_appearance(&self.values.string("appearance"));
    }
}

fn apply_appearance(value: &str) {
    let color_scheme = match value {
        "light" => adw::ColorScheme::ForceLight,
        "dark" => adw::ColorScheme::ForceDark,
        _ => adw::ColorScheme::Default,
    };
    adw::StyleManager::default().set_color_scheme(color_scheme);
}
