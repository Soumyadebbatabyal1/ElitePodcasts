use adw::prelude::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use crate::discovery_service::{self, Show};
use crate::following::FollowingStore;
use crate::opml;

pub(super) fn install_actions(
    window: &adw::ApplicationWindow,
    app: &adw::Application,
    store: Rc<FollowingStore>,
    toast: Rc<dyn Fn(&str)>,
) {
    let import = gtk::gio::SimpleAction::new("import-shows", None);
    let window_for_import = window.downgrade();
    let store_for_import = store.clone();
    let toast_for_import = toast.clone();
    import.connect_activate(move |_, _| {
        if let Some(window) = window_for_import.upgrade() {
            choose_import(&window, store_for_import.clone(), toast_for_import.clone());
        }
    });
    window.add_action(&import);
    app.set_accels_for_action("win.import-shows", &["<Primary>o"]);

    let export = gtk::gio::SimpleAction::new("export-shows", None);
    let window_for_export = window.downgrade();
    export.connect_activate(move |_, _| {
        if let Some(window) = window_for_export.upgrade() {
            choose_export(&window, store.clone(), toast.clone());
        }
    });
    window.add_action(&export);
    app.set_accels_for_action("win.export-shows", &["<Primary>e"]);
}

fn chooser(window: &adw::ApplicationWindow, action: gtk::FileChooserAction, title: &str, accept: &str) -> gtk::FileChooserNative {
    let dialog = gtk::FileChooserNative::new(
        Some(title), Some(window), action, Some(accept), Some("Cancel"),
    );
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("OPML files"));
    filter.add_pattern("*.opml");
    filter.add_pattern("*.OPML");
    filter.add_pattern("*.xml");
    dialog.add_filter(&filter);
    let all = gtk::FileFilter::new();
    all.set_name(Some("All files"));
    all.add_pattern("*");
    dialog.add_filter(&all);
    dialog.set_filter(&filter);
    dialog
}

fn choose_import(window: &adw::ApplicationWindow, store: Rc<FollowingStore>, toast: Rc<dyn Fn(&str)>) {
    let dialog = chooser(window, gtk::FileChooserAction::Open, "Import Shows", "Import");
    dialog.connect_response(move |dialog, response| {
        if response != gtk::ResponseType::Accept { return }
        let Some(file) = dialog.file() else { return };
        let store = store.clone();
        let toast = toast.clone();
        gtk::glib::MainContext::default().spawn_local(async move {
            let contents = match file.load_contents_future().await {
                Ok((contents, _)) => contents,
                Err(error) => {
                    toast(&format!("Could not open the OPML file: {error}"));
                    return;
                }
            };
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(opml::parse(contents.as_ref()));
            });
            gtk::glib::timeout_add_local(Duration::from_millis(50), move || {
                match rx.try_recv() {
                    Ok(Ok(list)) => {
                        if list.shows.is_empty() {
                            toast("No usable podcast feeds were found in this OPML file.");
                        } else {
                            let total = list.shows.len();
                            match store.add_many(&list.shows) {
                                Ok(inserted) => {
                                    let added = inserted.len();
                                    let existing = total - added;
                                    let mut message = format!("Imported {added} shows");
                                    if existing > 0 { message.push_str(&format!(", {existing} already followed")); }
                                    if list.skipped > 0 { message.push_str(&format!(", {} skipped", list.skipped)); }
                                    toast(&message);
                                    enrich_in_background(store.clone(), inserted, toast.clone());
                                }
                                Err(error) => toast(&error),
                            }
                        }
                        gtk::glib::ControlFlow::Break
                    }
                    Ok(Err(error)) => {
                        toast(&error);
                        gtk::glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        toast("Could not read the OPML file.");
                        gtk::glib::ControlFlow::Break
                    }
                }
            });
        });
    });
    dialog.show();
}

fn enrich_in_background(store: Rc<FollowingStore>, shows: Vec<Show>, toast: Rc<dyn Fn(&str)>) {
    if shows.is_empty() { return }
    let jobs = Arc::new(Mutex::new(VecDeque::from(shows)));
    let (tx, rx) = mpsc::channel::<(String, String, Option<String>)>();
    for _ in 0..4 {
        let jobs = jobs.clone();
        let tx = tx.clone();
        std::thread::spawn(move || loop {
            let next = jobs.lock().expect("OPML work queue poisoned").pop_front();
            let Some(show) = next else { break };
            if let Ok(details) = discovery_service::feed(&show.feed_url) {
                if tx.send((show.feed_url, details.title, details.artwork_url)).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    let pending = Rc::new(RefCell::new(Vec::new()));
    gtk::glib::timeout_add_local(Duration::from_millis(150), move || {
        let mut done = false;
        while pending.borrow().len() < 16 {
            match rx.try_recv() {
                Ok(details) => pending.borrow_mut().push(details),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => { done = true; break; }
            }
        }
        let batch = std::mem::take(&mut *pending.borrow_mut());
        if let Err(error) = store.enrich_imported(&batch) {
            toast(&error);
            return gtk::glib::ControlFlow::Break;
        }
        if done { gtk::glib::ControlFlow::Break } else { gtk::glib::ControlFlow::Continue }
    });
}

fn choose_export(window: &adw::ApplicationWindow, store: Rc<FollowingStore>, toast: Rc<dyn Fn(&str)>) {
    let dialog = chooser(window, gtk::FileChooserAction::Save, "Export Shows", "Export");
    dialog.set_current_name("elite-podcasts.opml");
    dialog.connect_response(move |dialog, response| {
        if response != gtk::ResponseType::Accept { return }
        let Some(file) = dialog.file() else { return };
        let shows = match store.list() {
            Ok(shows) => shows,
            Err(error) => { toast(&error); return; }
        };
        let contents = opml::export(&shows).into_bytes();
        let toast = toast.clone();
        gtk::glib::MainContext::default().spawn_local(async move {
            match file.replace_contents_future(
                contents, None, false, gtk::gio::FileCreateFlags::NONE,
            ).await {
                Ok(_) => toast(&format!("Exported {} shows", shows.len())),
                Err((_, error)) => toast(&format!("Could not export shows: {error}")),
            }
        });
    });
    dialog.show();
}
