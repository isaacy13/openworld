// SPDX-License-Identifier: Apache-2.0

//! The window: three pages in a navigation view, over the view-model.
//!
//! Clicks call the model. The jobs it returns run on worker threads, and their
//! messages come back to the main loop through a channel, so the window never
//! waits on the scan library.

pub(crate) mod dialogs;
mod review;
mod scan;
mod start;
pub mod style;
mod widgets;

use adw::prelude::*;
use gtk::{gio, glib};
use openworld_core::copy;
use openworld_core::Line;
use openworld_desktop::calls::{self, Job, Msg};
use openworld_desktop::model::{Back, Model, Phase};
use openworld_desktop::places::{self, Places};
use openworld_desktop::strings::{self, S};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

pub const APP_ID: &str = "app.openworld.OpenWorld";
const TAGS: [&str; 3] = ["start", "review", "scan"];

pub struct Ui {
    pub app: adw::Application,
    pub window: adw::ApplicationWindow,
    pub nav: adw::NavigationView,
    pub toasts: adw::ToastOverlay,
    start: start::Start,
    review: review::Review,
    scan: scan::Scan,
    pub model: RefCell<Model>,
    tx: async_channel::Sender<Msg>,
    hold: RefCell<Option<gio::ApplicationHoldGuard>>,
    syncing: Cell<bool>,
}

impl Ui {
    pub fn new(app: &adw::Application) -> Rc<Self> {
        let bundles = places::find_bundles(std::env::var_os("OPENWORLD_BUNDLES").map(PathBuf::from), std::env::current_exe().ok());
        let model = Model::new(Places::new(&glib::user_data_dir(), bundles));
        let (tx, rx) = async_channel::unbounded();

        // Phone width: 16 px gutters instead of 24.
        let narrow = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            500.0,
            adw::LengthUnit::Sp,
        ));
        let start = start::Start::build(&narrow);
        let review = review::Review::build(&narrow);
        let scan = scan::Scan::build(&narrow);
        let nav = adw::NavigationView::new();
        nav.add(&start.page);
        nav.add(&review.page);
        nav.add(&scan.page);
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&nav));
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(strings::text(S::AppName))
            .default_width(880)
            .default_height(820)
            .width_request(360)
            .height_request(480)
            .content(&toasts)
            .build();
        window.add_breakpoint(narrow);

        let ui = Rc::new(Self {
            app: app.clone(),
            window,
            nav,
            toasts,
            start,
            review,
            scan,
            model: RefCell::new(model),
            tx,
            hold: RefCell::new(None),
            syncing: Cell::new(false),
        });
        ui.start.wire(&ui);
        ui.review.wire(&ui);
        ui.scan.wire(&ui);
        ui.wire_window();

        let weak = Rc::downgrade(&ui);
        glib::spawn_future_local(async move {
            while let Ok(msg) = rx.recv().await {
                let Some(ui) = weak.upgrade() else { break };
                ui.dispatch(msg);
            }
        });
        let jobs = ui.model.borrow_mut().startup();
        jobs.into_iter().for_each(|job| ui.spawn(job));
        ui.render();
        ui
    }

    fn wire_window(self: &Rc<Self>) {
        let open = gio::SimpleAction::new("open", None);
        let weak = Rc::downgrade(self);
        open.connect_activate(move |_, _| {
            if let Some(ui) = weak.upgrade() {
                ui.pick_file();
            }
        });
        self.window.add_action(&open);

        let back = gio::SimpleAction::new("back", None);
        let weak = Rc::downgrade(self);
        back.connect_activate(move |_, _| {
            if let Some(ui) = weak.upgrade() {
                ui.go_back();
            }
        });
        self.window.add_action(&back);

        // Escape goes back. Dialogs see it first and close themselves.
        let keys = gtk::ShortcutController::new();
        keys.add_shortcut(gtk::Shortcut::new(gtk::ShortcutTrigger::parse_string("Escape"), Some(gtk::NamedAction::new("win.back"))));
        self.window.add_controller(keys);

        let weak = Rc::downgrade(self);
        self.nav.connect_popped(move |_, _| {
            if let Some(ui) = weak.upgrade() {
                ui.on_popped();
            }
        });

        let weak = Rc::downgrade(self);
        self.window.connect_close_request(move |window| {
            let Some(ui) = weak.upgrade() else { return glib::Propagation::Proceed };
            if ui.model.borrow_mut().close() {
                return glib::Propagation::Proceed;
            }
            // Wait for the stopped scan to hand back its folder, then remove it and quit.
            window.set_visible(false);
            *ui.hold.borrow_mut() = Some(ui.app.hold());
            glib::Propagation::Stop
        });
    }

    /// Run a job on its own thread. Its messages come back to `dispatch`.
    pub fn spawn(&self, job: Job) {
        let tx = self.tx.clone();
        let places = self.model.borrow().places.clone();
        std::thread::spawn(move || calls::run(job, &places, &mut |msg| drop(tx.send_blocking(msg))));
    }

    fn dispatch(self: &Rc<Self>, msg: Msg) {
        let was_scanning = self.model.borrow().scanning();
        let toast = match &msg {
            Msg::Deleted(Ok(line)) => Some(strings::say(line).to_string()),
            Msg::Posters(Err(line)) if matches!(self.model.borrow().pack, openworld_desktop::model::PackState::Installing) => {
                Some(strings::say(line).to_string())
            }
            _ => None,
        };
        let jobs = self.model.borrow_mut().apply(msg);
        jobs.into_iter().for_each(|job| self.spawn(job));
        let (closing, can_quit) = {
            let model = self.model.borrow();
            (model.closing, model.can_quit)
        };
        if closing {
            // Only a close that waited for a scan holds the app; the window goes once.
            if can_quit && self.hold.borrow_mut().take().is_some() {
                self.window.destroy();
            }
            return;
        }
        if let Some(text) = toast {
            self.toast(&text);
        }
        self.render();
        if was_scanning && matches!(self.model.borrow().phase, Phase::Result(_)) {
            self.scan.announce(self);
        }
    }

    pub fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::new(text));
    }

    /// Bring the pages in line with the model, then refresh the visible one.
    pub fn render(self: &Rc<Self>) {
        let want = phase_index(&self.model.borrow().phase);
        let current = self.visible_index();
        if want != current {
            self.syncing.set(true);
            if want < current {
                self.nav.pop_to_tag(TAGS[want]);
            } else {
                for tag in &TAGS[current + 1..=want] {
                    self.nav.push_by_tag(tag);
                }
            }
            self.syncing.set(false);
        }
        match want {
            0 => self.start.update(self),
            1 => self.review.update(self),
            _ => self.scan.update(self),
        }
        if want != current {
            match want {
                0 => self.start.focus(),
                1 => self.review.focus(),
                _ => self.scan.focus(),
            }
        }
    }

    fn visible_index(&self) -> usize {
        let tag = self.nav.visible_page().and_then(|p| p.tag()).map(|t| t.to_string()).unwrap_or_default();
        TAGS.iter().position(|t| *t == tag).unwrap_or(0)
    }

    /// The person went back with the header button, a gesture, or a key.
    fn on_popped(self: &Rc<Self>) {
        if self.syncing.get() {
            return;
        }
        let deeper = phase_index(&self.model.borrow().phase) > self.visible_index();
        if deeper {
            self.model.borrow_mut().back();
        }
        self.render();
    }

    pub fn go_back(self: &Rc<Self>) {
        if self.model.borrow().scanning() {
            if self.model.borrow_mut().back() == Back::AskStop {
                dialogs::confirm_stop(self);
            }
            return;
        }
        if self.visible_index() > 0 {
            self.nav.pop();
        }
    }

    pub fn pick_file(self: &Rc<Self>) {
        if self.model.borrow().scanning() {
            return;
        }
        let media = gtk::FileFilter::new();
        media.set_name(Some(strings::text(S::FileFilterMedia)));
        media.add_mime_type("image/*");
        media.add_mime_type("video/*");
        // Windows has no MIME database; these suffixes are the same set.
        for suffix in [
            "jpg", "jpeg", "png", "webp", "gif", "tif", "tiff", "heic", "heif", "avif", "mp4", "m4v", "mov", "mkv", "webm", "avi", "3gp",
            "ogv",
        ] {
            media.add_suffix(suffix);
        }
        let all = gtk::FileFilter::new();
        all.set_name(Some(strings::text(S::FileFilterAll)));
        all.add_pattern("*");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&media);
        filters.append(&all);
        let dialog =
            gtk::FileDialog::builder().title(strings::text(S::ChooseFile)).modal(true).filters(&filters).default_filter(&media).build();
        let ui = self.clone();
        glib::spawn_future_local(async move {
            if let Ok(file) = dialog.open_future(Some(&ui.window)).await {
                ui.open_file(file);
            }
        });
    }

    /// Open a picked, dropped, or passed file. One with no local path is copied first.
    pub fn open_file(self: &Rc<Self>, file: gio::File) {
        if self.model.borrow().scanning() || self.model.borrow().closing {
            return;
        }
        if let Some(path) = file.path() {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            self.choose(path, name, None, false);
            return;
        }
        let ui = self.clone();
        let folder = self.model.borrow().places.imports.clone();
        glib::spawn_future_local(async move {
            match import(&file, &folder).await {
                Some((path, name, mtime)) => ui.choose(path, name, mtime, true),
                None => ui.toast(strings::say(&Line::of(copy::R_UNREADABLE))),
            }
        });
    }

    fn choose(self: &Rc<Self>, path: PathBuf, name: String, mtime: Option<i64>, imported: bool) {
        let jobs = self.model.borrow_mut().choose(path, name, mtime, imported);
        jobs.into_iter().for_each(|job| self.spawn(job));
        self.render();
    }

    pub fn analyze(self: &Rc<Self>) {
        let job = self.model.borrow_mut().analyze(SystemTime::now());
        if let Some(job) = job {
            self.spawn(job);
        }
        self.render();
    }

    pub fn stop(self: &Rc<Self>) {
        self.model.borrow_mut().stop();
        self.render();
    }

    pub fn reset(self: &Rc<Self>) {
        self.model.borrow_mut().reset();
        self.render();
    }

    pub fn install_demo(self: &Rc<Self>) {
        let job = self.model.borrow_mut().install_demo();
        if let Some(job) = job {
            self.spawn(job);
        }
        self.render();
    }

    pub fn update_posters(self: &Rc<Self>) {
        let job = self.model.borrow_mut().update_posters();
        if let Some(job) = job {
            self.spawn(job);
        }
        self.render();
    }

    pub fn delete_result(self: &Rc<Self>) {
        let job = self.model.borrow_mut().delete_result();
        if let Some(job) = job {
            self.spawn(job);
        }
    }

    /// Ask the core whether this address may open, then show the leaving sheet.
    pub fn leave(self: &Rc<Self>, url: &str, notes: &[Line]) {
        match calls::leave(url) {
            Ok((title, approved)) => dialogs::leaving(self, &title, notes, approved),
            Err(line) => self.toast(strings::say(&line)),
        }
    }
}

fn phase_index(phase: &Phase) -> usize {
    match phase {
        Phase::Start => 0,
        Phase::Review => 1,
        Phase::Scanning(_) | Phase::Result(_) => 2,
    }
}

/// Copy a file with no local path into this run's temporary folder, keeping its name and time.
async fn import(file: &gio::File, folder: &std::path::Path) -> Option<(PathBuf, String, Option<i64>)> {
    let info =
        file.query_info_future("standard::display-name,time::modified", gio::FileQueryInfoFlags::NONE, glib::Priority::DEFAULT).await.ok();
    let name = info
        .as_ref()
        .map(|i| i.display_name().to_string())
        .or_else(|| file.basename().map(|b| b.to_string_lossy().to_string()))
        .map(|n| std::path::Path::new(&n).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default())
        .filter(|n| !n.is_empty() && n != "." && n != "..")
        .unwrap_or_else(|| "file".into());
    let mtime = info.and_then(|i| i.modification_date_time()).map(|t| t.to_unix());
    std::fs::create_dir_all(folder).ok()?;
    let mut dest = folder.join(&name);
    let mut n = 2;
    while dest.exists() {
        dest = folder.join(format!("{n}-{name}"));
        n += 1;
    }
    let (copied, _progress) = file.copy_future(&gio::File::for_path(&dest), gio::FileCopyFlags::ALL_METADATA, glib::Priority::DEFAULT);
    if copied.await.is_err() {
        let _ = std::fs::remove_file(&dest);
        return None;
    }
    Some((dest, name, mtime))
}

/// What `main` asked for.
#[derive(Clone, Debug)]
pub enum Mode {
    Normal,
    /// Build the window, wait for startup, quit. For CI.
    SelfCheck,
    /// Drive every screen and save PNGs here. A dev build only.
    #[cfg_attr(not(feature = "shots"), allow(dead_code))]
    Shots(PathBuf),
}

pub fn run(mode: Mode, files: Vec<String>) -> glib::ExitCode {
    let mut flags = gio::ApplicationFlags::HANDLES_OPEN;
    if !matches!(mode, Mode::Normal) {
        flags |= gio::ApplicationFlags::NON_UNIQUE;
    }
    let app = adw::Application::builder().application_id(APP_ID).flags(flags).build();
    app.connect_startup(|app| {
        style::install();
        app.set_accels_for_action("win.open", &["<Control>o"]);
        app.set_accels_for_action("window.close", &["<Control>w"]);
    });
    let slot: Rc<RefCell<Option<Rc<Ui>>>> = Rc::default();
    let status: Rc<Cell<i32>> = Rc::default();
    let ensure = {
        let slot = slot.clone();
        let status = status.clone();
        move |app: &adw::Application| -> Rc<Ui> {
            if let Some(ui) = slot.borrow().as_ref() {
                return ui.clone();
            }
            let ui = Ui::new(app);
            *slot.borrow_mut() = Some(ui.clone());
            match &mode {
                Mode::Normal => {}
                Mode::SelfCheck => self_check(&ui, status.clone()),
                #[cfg(feature = "shots")]
                Mode::Shots(dir) => crate::shots::run(ui.clone(), dir.clone(), status.clone()),
                #[cfg(not(feature = "shots"))]
                Mode::Shots(_) => {
                    eprintln!("--screenshot needs a build with --features shots");
                    status.set(2);
                    app.quit();
                }
            }
            ui
        }
    };
    let on_activate = ensure.clone();
    app.connect_activate(move |app| on_activate(app).window.present());
    app.connect_open(move |app, files, _| {
        let ui = ensure(app);
        ui.window.present();
        if let Some(file) = files.first() {
            ui.open_file(file.clone());
        }
    });
    let mut argv = vec!["openworld-desktop".to_string()];
    argv.extend(files);
    app.run_with_args(&argv);
    glib::ExitCode::from(status.get().clamp(0, 255) as u8)
}

/// The window was built and startup read the bundles and the posters.
fn self_check(ui: &Rc<Ui>, status: Rc<Cell<i32>>) {
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        for _ in 0..200 {
            glib::timeout_future(std::time::Duration::from_millis(50)).await;
            let ready = {
                let model = ui.model.borrow();
                model.bundles.is_some() && !matches!(model.pack, openworld_desktop::model::PackState::Checking)
            };
            if ready && ui.window.is_mapped() {
                let model = ui.model.borrow();
                let bundles = match &model.bundles {
                    Some(Ok(rows)) => rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>().join(","),
                    Some(Err(line)) => strings::say(line).to_string(),
                    None => String::new(),
                };
                println!("self-check ok: window shown; bundles: {bundles}");
                drop(model);
                ui.window.close();
                return;
            }
        }
        eprintln!("self-check failed: the window or startup did not finish");
        status.set(1);
        ui.app.quit();
    });
}
