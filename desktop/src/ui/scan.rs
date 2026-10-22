// SPDX-License-Identifier: Apache-2.0

//! Scan and Result, one screen: progress and the faces as they are found,
//! then the headline, candidate cards, details, other faces, and the disclosure.

use super::widgets::{self, card, clear, dim, hbox, heading, icon_line, text, vbox, FaceTile};
use super::{dialogs, Ui};
use adw::prelude::*;
use gtk::glib;
use openworld_core::report::{Candidate, ScanReport};
use openworld_core::Line;
use openworld_desktop::model::{self, Done, Face, Phase, Text, Tone};
use openworld_desktop::strings::{self, S};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

const FACE: i32 = 96;
/// Two of these side by side fit a 360 px window.
const CARD_PICTURE: i32 = 136;

pub struct Scan {
    pub page: adw::NavigationPage,
    title: adw::WindowTitle,
    banner: adw::Banner,
    stack: gtk::Stack,
    progress: gtk::ProgressBar,
    progress_label: gtk::Label,
    faces: gtk::FlowBox,
    faces_empty: gtk::Label,
    stop: gtk::Button,
    tiles: RefCell<HashMap<u64, (FaceTile, Option<openworld_core::TrackStatus>)>>,
    scanning_out: RefCell<Option<PathBuf>>,
    result_box: gtk::Box,
    drawn_result: RefCell<Option<(PathBuf, bool)>>,
    another: RefCell<Option<gtk::Button>>,
}

impl Scan {
    pub fn build(narrow: &adw::Breakpoint) -> Self {
        let scanning = vbox(18);
        let status = card(12);
        let lock = icon_line("channel-secure-symbolic", strings::text(S::ScanningOnDevice));
        lock.add_css_class("heading");
        let progress = gtk::ProgressBar::new();
        progress.set_accessible_role(gtk::AccessibleRole::ProgressBar);
        let progress_label = dim("");
        status.append(&lock);
        status.append(&progress);
        status.append(&progress_label);
        let faces = flow();
        let faces_empty = dim(strings::text(S::NoFacesYet));
        let stop = widgets::pill(strings::text(S::Stop), None);
        scanning.append(&status);
        scanning.append(&heading(strings::text(S::Faces)));
        scanning.append(&faces_empty);
        scanning.append(&faces);
        scanning.append(&stop);

        let result_box = vbox(18);
        let stack = gtk::Stack::builder().transition_type(gtk::StackTransitionType::Crossfade).vhomogeneous(false).build();
        stack.add_named(&widgets::scroller(&scanning, 720, narrow), Some("scanning"));
        stack.add_named(&widgets::scroller(&result_box, 720, narrow), Some("result"));

        let title = adw::WindowTitle::new(strings::text(S::Scanning), "");
        let header = adw::HeaderBar::builder().title_widget(&title).build();
        let (page, toolbar) = widgets::page(strings::text(S::Scanning), "scan", &header, &stack);
        let banner = adw::Banner::new("");
        toolbar.add_top_bar(&banner);
        Self {
            page,
            title,
            banner,
            stack,
            progress,
            progress_label,
            faces,
            faces_empty,
            stop,
            tiles: RefCell::default(),
            scanning_out: RefCell::default(),
            result_box,
            drawn_result: RefCell::default(),
            another: RefCell::default(),
        }
    }

    pub fn wire(&self, ui: &Rc<Ui>) {
        let weak = Rc::downgrade(ui);
        self.stop.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.stop();
            }
        });
    }

    pub fn update(&self, ui: &Rc<Ui>) {
        let model = ui.model.borrow();
        match &model.phase {
            Phase::Scanning(scan) => {
                self.page.set_can_pop(false);
                self.title.set_title(strings::text(S::Scanning));
                self.page.set_title(strings::text(S::Scanning));
                self.stack.set_visible_child_name("scanning");
                self.banner.set_title(strings::say(&Line::of(openworld_core::copy::BRIEF_FACE)));
                self.banner.set_revealed(scan.coverage == openworld_core::Coverage::Measured);
                if self.scanning_out.borrow().as_ref() != Some(&scan.out) {
                    *self.scanning_out.borrow_mut() = Some(scan.out.clone());
                    self.tiles.borrow_mut().clear();
                    self.faces.remove_all();
                }
                self.progress.set_fraction(scan.analyzed as f64 / scan.total.max(1) as f64);
                let mut words = strings::progress(scan.analyzed, scan.total);
                if let Some(left) = model.time_left(Instant::now()) {
                    words.push_str(" · ");
                    words.push_str(&strings::time_left(&left));
                }
                self.progress_label.set_text(&words);
                self.stop.set_sensitive(!scan.stopping);
                self.stop.set_label(strings::text(if scan.stopping { S::Stopping } else { S::Stop }));
                for face in &scan.faces {
                    self.show_face(face);
                }
                self.faces_empty.set_visible(scan.faces.is_empty());
            }
            Phase::Result(done) => {
                self.page.set_can_pop(true);
                self.title.set_title(strings::text(S::Result));
                self.page.set_title(strings::text(S::Result));
                self.stack.set_visible_child_name("result");
                match &done.report.coverage_banner {
                    Some(line) => {
                        self.banner.set_title(strings::say(line));
                        self.banner.set_revealed(true);
                    }
                    None => self.banner.set_revealed(false),
                }
                let key = (done.out.clone(), done.delete_error.is_some());
                if self.drawn_result.borrow().as_ref() != Some(&key) {
                    *self.drawn_result.borrow_mut() = Some(key);
                    self.draw_result(ui, done);
                }
            }
            _ => {}
        }
    }

    /// Add a face, or update its chip when its track is decided. New faces fade in.
    fn show_face(&self, face: &Face) {
        let mut tiles = self.tiles.borrow_mut();
        if let Some((tile, status)) = tiles.get_mut(&face.track_id) {
            if *status != face.status {
                *status = face.status;
                tile.set_crop(face.crop.as_deref(), FACE);
                fill_tile(tile, face);
            }
            return;
        }
        let tile = FaceTile::new(FACE);
        tile.set_crop(face.crop.as_deref(), FACE);
        fill_tile(&tile, face);
        let reveal = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .transition_duration(250)
            .child(&tile.root)
            .build();
        self.faces.append(&reveal);
        glib::idle_add_local_once(move || reveal.set_reveal_child(true));
        tiles.insert(face.track_id, (tile, face.status));
    }

    fn draw_result(&self, ui: &Rc<Ui>, done: &Done) {
        clear(&self.result_box);
        let report = &done.report;
        self.result_box.append(&headline(report));

        if let Some(line) = &done.delete_error {
            self.result_box.append(&widgets::tinted(widgets::tone_icon(Tone::Error), strings::say(line), Tone::Error));
        }

        if !report.candidates.is_empty() {
            self.result_box.append(&heading(strings::text(S::Candidates)));
            for candidate in &report.candidates {
                self.result_box.append(&candidate_card(ui, &done.out, candidate));
            }
        }

        if !report.is_refused() {
            self.result_box.append(&details(report));
            let others = model::other_faces(done);
            if !others.is_empty() {
                self.result_box.append(&heading(strings::text(S::OtherFaces)));
                let grid = flow();
                for face in &others {
                    let tile = FaceTile::new(FACE);
                    tile.set_crop(face.crop.as_deref(), FACE);
                    fill_tile(&tile, face);
                    grid.append(&tile.root);
                }
                self.result_box.append(&grid);
            }
        }

        let disclosure = card(6);
        disclosure.append(&heading(strings::text(S::Disclosure)));
        for line in &report.disclosure {
            disclosure.append(&icon_line("channel-secure-symbolic", strings::say(line)));
        }
        self.result_box.append(&disclosure);

        let actions = hbox(12);
        actions.set_halign(gtk::Align::Center);
        actions.set_margin_top(6);
        if !report.is_refused() {
            let delete = widgets::pill(strings::text(S::DeleteResult), Some("destructive-action"));
            let weak = Rc::downgrade(ui);
            delete.connect_clicked(move |_| {
                if let Some(ui) = weak.upgrade() {
                    dialogs::confirm_delete(&ui);
                }
            });
            actions.append(&delete);
        }
        let another = widgets::pill(strings::text(S::ScanAnother), Some("suggested-action"));
        let weak = Rc::downgrade(ui);
        another.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.reset();
            }
        });
        actions.append(&another);
        self.result_box.append(&actions);
        *self.another.borrow_mut() = Some(another);
    }

    /// Say the headline to a screen reader when the result appears.
    pub fn announce(&self, ui: &Rc<Ui>) {
        if let Phase::Result(done) = &ui.model.borrow().phase {
            let mut words = vec![strings::say(&done.report.headline).to_string()];
            words.extend(model::headline_detail(&done.report).iter().map(Text::show));
            self.result_box.announce(&words.join(" "), gtk::AccessibleAnnouncementPriority::High);
        }
        self.focus();
    }

    pub fn focus(&self) {
        if let Some(button) = self.another.borrow().as_ref().filter(|_| self.stack.visible_child_name().as_deref() == Some("result")) {
            button.grab_focus();
        } else {
            self.stop.grab_focus();
        }
    }
}

fn flow() -> gtk::FlowBox {
    gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .homogeneous(true)
        .min_children_per_line(2)
        .max_children_per_line(8)
        .column_spacing(6)
        .row_spacing(6)
        .valign(gtk::Align::Start)
        .build()
}

fn fill_tile(tile: &FaceTile, face: &Face) {
    widgets::set_chip(&tile.chip, face.chip(), face.tone());
    let caption = match (&face.reason, &face.label) {
        (Some(reason), _) => strings::say(reason).to_string(),
        _ => strings::say(&openworld_core::copy::frame_label(face.frame)).to_string(),
    };
    tile.caption.set_text(&caption);
    let label = face.accessible_label();
    tile.root.update_property(&[gtk::accessible::Property::Label(&label)]);
    tile.root.set_tooltip_text(Some(&label));
}

/// The headline card, colored by state, with its icon and the lines under it.
fn headline(report: &ScanReport) -> gtk::Box {
    let tone = model::tone(report);
    let card = vbox(8);
    card.add_css_class("ow-headline");
    card.add_css_class(widgets::tone_class(tone));
    let row = hbox(12);
    let icon = gtk::Image::from_icon_name(widgets::tone_icon(tone));
    icon.set_pixel_size(28);
    icon.set_valign(gtk::Align::Start);
    let title = text(strings::say(&report.headline));
    title.add_css_class("ow-headline-title");
    title.set_accessible_role(gtk::AccessibleRole::Heading);
    title.set_hexpand(true);
    row.append(&icon);
    row.append(&title);
    card.append(&row);
    for line in model::headline_detail(report) {
        let label = text(&line.show());
        label.set_margin_start(40);
        card.append(&label);
    }
    for warning in &report.warnings {
        let label = text(strings::say(warning));
        label.set_margin_start(40);
        label.add_css_class("caption");
        card.append(&label);
    }
    card
}

/// The crop beside the poster, the band, the frame, and the way out.
fn candidate_card(ui: &Rc<Ui>, out: &Path, candidate: &Candidate) -> gtk::Box {
    let card = card(12);
    card.add_css_class("ow-candidate-card");
    let pictures = hbox(12);
    pictures.set_homogeneous(true);
    for (path, caption, icon) in [
        (candidate.crop.as_ref().map(|c| out.join(c)), S::FileFrom, "avatar-default-symbolic"),
        (candidate.poster.image.as_ref().map(PathBuf::from), S::Poster, "image-x-generic-symbolic"),
    ] {
        let column = vbox(6);
        let picture = widgets::picture(path.as_deref(), CARD_PICTURE, icon);
        let words = format!("{}. {}", strings::text(caption), candidate.poster.title);
        picture.update_property(&[gtk::accessible::Property::Label(&words)]);
        let label = dim(strings::text(caption));
        label.set_xalign(0.5);
        label.add_css_class("caption");
        column.append(&picture);
        column.append(&label);
        pictures.append(&column);
    }
    card.append(&pictures);

    let title_row = hbox(10);
    let title = widgets::name(&candidate.poster.title);
    title.add_css_class("title-4");
    title.set_hexpand(true);
    let badge = gtk::Label::new(Some(&candidate.poster.class_label));
    badge.add_css_class("ow-badge");
    badge.add_css_class(widgets::tone_class(Tone::Neutral));
    badge.set_valign(gtk::Align::Center);
    title_row.append(&title);
    title_row.append(&badge);
    card.append(&title_row);
    card.append(&widgets::tinted(widgets::tone_icon(Tone::Candidate), strings::say(&candidate.band), Tone::Candidate));

    let frame_row = hbox(8);
    let frame_label = dim(strings::say(&candidate.frame_label));
    frame_label.set_hexpand(true);
    frame_label.set_valign(gtk::Align::Center);
    let show = gtk::Button::with_label(strings::text(S::ShowFrame));
    show.add_css_class("flat");
    let frame = candidate.frame.as_ref().map(|f| out.join(f));
    show.set_sensitive(frame.as_ref().is_some_and(|f| f.is_file()));
    let weak = Rc::downgrade(ui);
    let label = candidate.frame_label.clone();
    show.connect_clicked(move |_| {
        if let (Some(ui), Some(frame)) = (weak.upgrade(), frame.as_ref()) {
            dialogs::show_frame(&ui, frame, &label);
        }
    });
    frame_row.append(&frame_label);
    frame_row.append(&show);
    card.append(&frame_row);

    let open = gtk::Button::new();
    let content = adw::ButtonContent::builder().label(strings::text(S::OpenFbi)).icon_name("web-browser-symbolic").build();
    open.set_child(Some(&content));
    open.add_css_class("pill");
    open.set_halign(gtk::Align::Start);
    match candidate.poster.fbi_url.clone() {
        Some(url) => {
            let weak = Rc::downgrade(ui);
            let notes = candidate.leave_notes.clone();
            open.connect_clicked(move |_| {
                if let Some(ui) = weak.upgrade() {
                    ui.leave(&url, &notes);
                }
            });
        }
        None => {
            open.set_sensitive(false);
            open.set_tooltip_text(Some(strings::text(S::NoFbiPage)));
        }
    }
    card.append(&open);
    card
}

/// Frames, coverage, detection size, chance, classes, bundle, posters, device.
fn details(report: &ScanReport) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    let expander = adw::ExpanderRow::builder().title(strings::text(S::Details)).build();
    let mut rows: Vec<(S, String)> = Vec::new();
    let mut add = |title: S, line: &Option<Line>| {
        if let Some(line) = line {
            rows.push((title, strings::say(line).to_string()));
        }
    };
    add(S::DetailFrames, &report.frames_line);
    add(S::DetailCoverage, &report.coverage_line);
    add(S::DetailDetection, &report.detection_line);
    add(S::DetailChance, &report.chance);
    add(S::DetailClasses, &report.classes);
    add(S::DetailPerception, &report.perception_note);
    let weights: Vec<String> = report.bundle.weights.iter().map(|w| format!("{} {}: {}", w.role, w.name, w.sha256)).collect();
    let mut bundle = strings::bundle_name(&report.bundle.name, &report.bundle.version);
    for weight in weights {
        bundle.push('\n');
        bundle.push_str(&weight);
    }
    rows.push((S::DetailBundle, bundle));
    rows.push((S::DetailPosters, format!("{} · {}\nSHA-256 {}", report.posters.id, report.posters.source, report.posters.sha256)));
    let mut device = format!("{} ({})", report.provider, report.execution);
    if let Some(note) = &report.device_note {
        device.push('\n');
        device.push_str(strings::say(note));
    }
    rows.push((S::DetailDevice, device));
    for (title, value) in rows {
        let row = adw::ActionRow::builder().title(strings::text(title)).subtitle(glib::markup_escape_text(&value).as_str()).build();
        row.set_subtitle_lines(0);
        row.set_subtitle_selectable(true);
        expander.add_row(&row);
    }
    group.add(&expander);
    group
}
