use adw::prelude::*;
use std::rc::Rc;

use crate::settings::{AppSettings, SKIP_LENGTHS, SPEEDS};

pub(super) fn present(
    window: &adw::ApplicationWindow,
    settings: Rc<AppSettings>,
    toast: Rc<dyn Fn(&str)>,
) {
    let dialog = adw::PreferencesDialog::new();
    dialog.set_title("Settings");

    let appearance_page = adw::PreferencesPage::new();
    appearance_page.set_title("Appearance");
    appearance_page.set_icon_name(Some("applications-graphics-symbolic"));
    let appearance_group = adw::PreferencesGroup::new();
    let appearance = combo_row("Color Scheme", &["Follow System", "Light", "Dark"]);
    appearance.set_selected(match settings.appearance().as_str() {
        "light" => 1,
        "dark" => 2,
        _ => 0,
    });
    let settings_for_appearance = settings.clone();
    let toast_for_appearance = toast.clone();
    appearance.connect_selected_notify(move |row| {
        let value = match row.selected() { 1 => "light", 2 => "dark", _ => "system" };
        if let Err(error) = settings_for_appearance.set_appearance(value) {
            toast_for_appearance(&error);
        }
    });
    appearance_group.add(&appearance);
    appearance_page.add(&appearance_group);
    dialog.add(&appearance_page);

    let playback_page = adw::PreferencesPage::new();
    playback_page.set_title("Playback");
    playback_page.set_icon_name(Some("media-playback-start-symbolic"));
    let playback_group = adw::PreferencesGroup::new();
    let speed = combo_row("Default Speed", &["0.75×", "1.00×", "1.25×", "1.50×", "1.75×", "2.00×"]);
    speed.set_subtitle("Used when a different episode starts");
    speed.set_selected(SPEEDS.iter().position(|value| *value == settings.default_speed()).unwrap_or(1) as u32);
    let settings_for_speed = settings.clone();
    let toast_for_speed = toast.clone();
    speed.connect_selected_notify(move |row| {
        if let Some(value) = SPEEDS.get(row.selected() as usize) {
            if let Err(error) = settings_for_speed.set_default_speed(*value) {
                toast_for_speed(&error);
            }
        }
    });
    playback_group.add(&speed);

    let lengths = ["5 seconds", "10 seconds", "15 seconds", "30 seconds"];
    let backward = combo_row("Skip Backward", &lengths);
    backward.set_selected(SKIP_LENGTHS.iter().position(|value| i64::from(*value) == settings.skip_backward_seconds()).unwrap_or(1) as u32);
    let settings_for_backward = settings.clone();
    let toast_for_backward = toast.clone();
    backward.connect_selected_notify(move |row| {
        if let Some(value) = SKIP_LENGTHS.get(row.selected() as usize) {
            if let Err(error) = settings_for_backward.set_skip_backward_seconds(*value) {
                toast_for_backward(&error);
            }
        }
    });
    playback_group.add(&backward);

    let forward = combo_row("Skip Forward", &lengths);
    forward.set_selected(SKIP_LENGTHS.iter().position(|value| i64::from(*value) == settings.skip_forward_seconds()).unwrap_or(1) as u32);
    let settings_for_forward = settings.clone();
    let toast_for_forward = toast.clone();
    forward.connect_selected_notify(move |row| {
        if let Some(value) = SKIP_LENGTHS.get(row.selected() as usize) {
            if let Err(error) = settings_for_forward.set_skip_forward_seconds(*value) {
                toast_for_forward(&error);
            }
        }
    });
    playback_group.add(&forward);
    playback_page.add(&playback_group);
    dialog.add(&playback_page);

    let storage_page = adw::PreferencesPage::new();
    storage_page.set_title("Storage");
    storage_page.set_icon_name(Some("drive-harddisk-symbolic"));
    let storage_group = adw::PreferencesGroup::new();
    let cache = adw::ActionRow::builder()
        .title("Clear Cache")
        .subtitle("Remove downloaded artwork and discovery data")
        .build();
    let clear = gtk::Button::with_label("Clear Cache");
    clear.set_valign(gtk::Align::Center);
    clear.add_css_class("destructive-action");
    cache.add_suffix(&clear);
    cache.set_activatable_widget(Some(&clear));
    let weak_dialog = dialog.downgrade();
    clear.connect_clicked(move |_| {
        let Some(dialog) = weak_dialog.upgrade() else { return };
        let confirmation = adw::AlertDialog::new(
            Some("Clear cached data?"),
            Some("Artwork and discovery data will download again when needed. Following, history, and playback positions are kept."),
        );
        confirmation.add_responses(&[("cancel", "Cancel"), ("clear", "Clear Cache")]);
        confirmation.set_close_response("cancel");
        confirmation.set_default_response(Some("cancel"));
        confirmation.set_response_appearance("clear", adw::ResponseAppearance::Destructive);
        let toast = toast.clone();
        confirmation.connect_response(None, move |_, response| {
            if response != "clear" { return; }
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(crate::cache::clear_all());
            });
            let toast = toast.clone();
            gtk::glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                match receiver.try_recv() {
                    Ok(Ok(())) => {
                        toast("Cache cleared");
                        gtk::glib::ControlFlow::Break
                    }
                    Ok(Err(error)) => {
                        toast(&error);
                        gtk::glib::ControlFlow::Break
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
                    Err(_) => gtk::glib::ControlFlow::Break,
                }
            });
        });
        confirmation.present(Some(&dialog));
    });
    storage_group.add(&cache);
    storage_page.add(&storage_group);
    dialog.add(&storage_page);
    dialog.present(Some(window));
}

fn combo_row(title: &str, choices: &[&str]) -> adw::ComboRow {
    let row = adw::ComboRow::new();
    row.set_title(title);
    row.set_model(Some(&gtk::StringList::new(choices)));
    row
}
