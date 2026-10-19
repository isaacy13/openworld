// SPDX-License-Identifier: Apache-2.0

//! Review: the file, the estimate, the classes, options, and what happens to the file.

use super::widgets::{self, card, clear, dim, hbox, heading, icon_line, text, vbox};
use super::{dialogs, Ui};
use adw::prelude::*;
use gtk::glib;
use openworld_core::copy;
use openworld_core::estimate::Coverage;
use openworld_core::report::disclosure;
use openworld_core::Line;
use openworld_desktop::model::{Block, EstimateState, Model, Tone};
use openworld_desktop::strings::{self, S};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

const THUMB: i32 = 104;
/// Disclosure lines shown before "Read all".
const FIRST_LINES: usize = 3;

pub struct Review {
    pub page: adw::NavigationPage,
    banner: adw::Banner,
    file_card: gtk::Box,
    estimate_card: gtk::Box,
    classes: gtk::Box,
    missing: gtk::ToggleButton,
    wanted: gtk::ToggleButton,
    class_hint: gtk::Label,
    options: adw::PreferencesGroup,
    expander: adw::ExpanderRow,
    bundle_rows: RefCell<Vec<(String, gtk::CheckButton, adw::ActionRow)>>,
    every_frame: gtk::CheckButton,
    five_a_second: gtk::CheckButton,
    read_all: gtk::Button,
    analyze: gtk::Button,
    another: gtk::Button,
    /// The file and whether its facts were in, last time the card was drawn.
    drawn_file: RefCell<Option<(PathBuf, bool)>>,
}

impl Review {
    pub fn build(narrow: &adw::Breakpoint) -> Self {
        let content = vbox(18);
        let file_card = card(12);
        let estimate_card = card(10);

        let classes = vbox(10);
        classes.append(&heading(strings::text(S::CompareWith)));
        let chips = hbox(8);
        let missing = gtk::ToggleButton::new();
        let wanted = gtk::ToggleButton::new();
        for chip in [&missing, &wanted] {
            chip.add_css_class("ow-toggle-chip");
            chips.append(chip);
        }
        let class_hint = dim(strings::say(&Line::of(copy::CHOOSE_CLASS)));
        classes.append(&chips);
        classes.append(&class_hint);

        let options = adw::PreferencesGroup::new();
        let expander = adw::ExpanderRow::builder().title(strings::text(S::Options)).build();
        options.add(&expander);
        let every_frame = gtk::CheckButton::new();
        let five_a_second = gtk::CheckButton::new();
        five_a_second.set_group(Some(&every_frame));

        let disclosure_card = card(8);
        disclosure_card.append(&heading(strings::text(S::WhatHappens)));
        for line in disclosure(true).iter().take(FIRST_LINES) {
            disclosure_card.append(&icon_line("channel-secure-symbolic", strings::say(line)));
        }
        let read_all = gtk::Button::with_label(strings::text(S::ReadAll));
        read_all.add_css_class("flat");
        read_all.set_halign(gtk::Align::Start);
        disclosure_card.append(&read_all);

        let actions = vbox(10);
        actions.set_margin_top(6);
        let analyze = widgets::pill(strings::text(S::Analyze), Some("suggested-action"));
        let another = widgets::pill(strings::text(S::ChooseAnother), Some("flat"));
        actions.append(&analyze);
        actions.append(&another);

        content.append(&file_card);
        content.append(&estimate_card);
        content.append(&classes);
        content.append(&options);
        content.append(&disclosure_card);
        content.append(&actions);

        let header = adw::HeaderBar::new();
        let scroller = widgets::scroller(&content, 640, narrow);
        let (page, toolbar) = widgets::page(strings::text(S::Review), "review", &header, &scroller);
        let banner = adw::Banner::new(strings::say(&Line::of(copy::BRIEF_FACE)));
        toolbar.add_top_bar(&banner);

        let review = Self {
            page,
            banner,
            file_card,
            estimate_card,
            classes,
            missing,
            wanted,
            class_hint,
            options,
            expander,
            bundle_rows: RefCell::default(),
            every_frame,
            five_a_second,
            read_all,
            analyze,
            another,
            drawn_file: RefCell::default(),
        };
        review.add_coverage_rows();
        review
    }

    fn add_coverage_rows(&self) {
        for (check, title, line) in
            [(&self.every_frame, S::EveryFrame, copy::COVERAGE_COMPLETE), (&self.five_a_second, S::FiveASecond, copy::BRIEF_FACE)]
        {
            let subtitle = glib::markup_escape_text(strings::say(&Line::of(line)));
            let row = adw::ActionRow::builder().title(strings::text(title)).subtitle(subtitle.as_str()).activatable_widget(check).build();
            row.add_prefix(check);
            row.set_subtitle_lines(0);
            self.expander.add_row(&row);
        }
    }

    pub fn wire(&self, ui: &Rc<Ui>) {
        for (chip, is_missing) in [(&self.missing, true), (&self.wanted, false)] {
            let weak = Rc::downgrade(ui);
            chip.connect_toggled(move |chip| {
                let Some(ui) = weak.upgrade() else { return };
                let (missing, wanted) = {
                    let choice = &ui.model.borrow().choice;
                    let next = if is_missing { (chip.is_active(), choice.wanted) } else { (choice.missing, chip.is_active()) };
                    if next == (choice.missing, choice.wanted) {
                        return;
                    }
                    next
                };
                ui.model.borrow_mut().set_classes(missing, wanted);
                ui.render();
            });
        }
        for (check, coverage) in [(&self.every_frame, Coverage::Complete), (&self.five_a_second, Coverage::Measured)] {
            let weak = Rc::downgrade(ui);
            check.connect_toggled(move |check| {
                let Some(ui) = weak.upgrade() else { return };
                if !check.is_active() {
                    return;
                }
                let job = ui.model.borrow_mut().set_coverage(coverage);
                if let Some(job) = job {
                    ui.spawn(job);
                    ui.render();
                }
            });
        }
        let weak = Rc::downgrade(ui);
        self.read_all.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                dialogs::read_all(&ui, &disclosure(true));
            }
        });
        let weak = Rc::downgrade(ui);
        self.analyze.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.analyze();
            }
        });
        let weak = Rc::downgrade(ui);
        self.another.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.reset();
                ui.pick_file();
            }
        });
    }

    pub fn update(&self, ui: &Rc<Ui>) {
        self.ensure_bundle_rows(ui);
        // Read what is needed and let go of the model: setting a toggle runs its handler.
        let (choice, counts, block, refused, bundle_name) = {
            let model = ui.model.borrow();
            self.draw_file(&model);
            self.draw_estimate(&model);
            let bundle_name = match &model.bundles {
                Some(Ok(rows)) => rows.iter().find(|r| r.id == model.choice.bundle).map(|r| r.name.clone()).unwrap_or_default(),
                _ => model.choice.bundle.clone(),
            };
            let refused = matches!(model.estimate, EstimateState::Refused(_));
            (model.choice.clone(), model.class_counts(), model.analyze_block(), refused, bundle_name)
        };

        // A refused estimate shows its sentence, not the classes or Analyze.
        self.classes.set_visible(!refused);
        self.options.set_visible(!refused);
        self.analyze.set_visible(!refused);
        self.missing.set_label(&strings::class_chip(S::Missing, counts.0));
        self.wanted.set_label(&strings::class_chip(S::Wanted, counts.1));
        self.missing.set_active(choice.missing);
        self.wanted.set_active(choice.wanted);
        self.class_hint.set_visible(block == Some(Block::NoClass));
        self.analyze.set_sensitive(block.is_none());

        for (id, check, _) in self.bundle_rows.borrow().iter() {
            if *id == choice.bundle {
                check.set_active(true);
            }
        }
        let measured = choice.coverage == Coverage::Measured;
        if measured { &self.five_a_second } else { &self.every_frame }.set_active(true);
        self.banner.set_revealed(measured && !refused);
        let coverage = strings::text(if measured { S::FiveASecond } else { S::EveryFrame });
        self.expander.set_subtitle(&format!("{bundle_name} · {coverage}"));
    }

    /// Bundle rows come from the core once, Fast first.
    fn ensure_bundle_rows(&self, ui: &Rc<Ui>) {
        if !self.bundle_rows.borrow().is_empty() {
            return;
        }
        let rows = match &ui.model.borrow().bundles {
            Some(Ok(rows)) => rows.clone(),
            _ => return,
        };
        let mut sorted = rows;
        sorted.sort_by_key(|r| !r.preselected);
        let mut group: Option<gtk::CheckButton> = None;
        let mut made = Vec::new();
        for row in &sorted {
            let check = gtk::CheckButton::new();
            if let Some(first) = &group {
                check.set_group(Some(first));
            } else {
                group = Some(check.clone());
            }
            let mut subtitle = format!("{}\n{}", row.best_for, strings::say(&row.curve_line));
            if !row.official {
                subtitle.push_str(&format!("\n{}", strings::text(S::Unofficial)));
            }
            let title = glib::markup_escape_text(&row.name);
            let action = adw::ActionRow::builder()
                .title(title.as_str())
                .subtitle(glib::markup_escape_text(&subtitle).as_str())
                .activatable_widget(&check)
                .build();
            action.set_subtitle_lines(0);
            action.add_prefix(&check);
            // Bundles first, then coverage.
            self.expander.add_row(&action);
            let weak = Rc::downgrade(ui);
            let id = row.id.clone();
            check.connect_toggled(move |check| {
                let Some(ui) = weak.upgrade() else { return };
                if !check.is_active() {
                    return;
                }
                let job = ui.model.borrow_mut().set_bundle(&id);
                if let Some(job) = job {
                    ui.spawn(job);
                    ui.render();
                }
            });
            made.push((row.id.clone(), check, action));
        }
        // Move the coverage rows after the bundles.
        self.reorder_coverage();
        *self.bundle_rows.borrow_mut() = made;
    }

    fn reorder_coverage(&self) {
        for check in [&self.every_frame, &self.five_a_second] {
            if let Some(row) = check.ancestor(adw::ActionRow::static_type()).and_downcast::<adw::ActionRow>() {
                self.expander.remove(&row);
                self.expander.add_row(&row);
            }
        }
    }

    fn draw_file(&self, model: &Model) {
        let Some(file) = &model.file else { return };
        let key = (file.path.clone(), file.facts.is_some());
        if self.drawn_file.borrow().as_ref() == Some(&key) {
            return;
        }
        *self.drawn_file.borrow_mut() = Some(key);
        clear(&self.file_card);
        let row = hbox(16);
        let facts = file.facts.as_ref().and_then(|f| f.as_ref().ok());
        let thumb = match facts {
            Some(facts) if facts.thumb.is_some() => widgets::thumb_picture(facts.thumb.as_ref().unwrap(), THUMB),
            Some(facts) if facts.video => widgets::picture(None, THUMB, "camera-video-symbolic"),
            _ => widgets::picture(None, THUMB, "image-x-generic-symbolic"),
        };
        let words = vbox(4);
        words.set_hexpand(true);
        words.set_valign(gtk::Align::Center);
        let name = widgets::name(&file.name);
        name.add_css_class("title-4");
        words.append(&name);
        match file.facts.as_ref() {
            None => words.append(&dim(strings::text(S::Estimating))),
            Some(Err(_)) => {}
            Some(Ok(facts)) => {
                words.append(&dim(&strings::file_kind(facts.video, facts.width, facts.height, facts.duration_sec, facts.bytes)));
                if let Some(date) =
                    facts.recorded_unix.and_then(|t| glib::DateTime::from_unix_local(t).ok()).and_then(|d| d.format("%x").ok())
                {
                    words.append(&dim(&strings::recorded(&date)));
                }
            }
        }
        row.append(&thumb);
        row.append(&words);
        self.file_card.append(&row);
        for warning in facts.map(|f| f.warnings.clone()).unwrap_or_default() {
            self.file_card.append(&widgets::tinted("dialog-warning-symbolic", strings::say(&warning), Tone::Neutral));
        }
    }

    fn draw_estimate(&self, model: &Model) {
        clear(&self.estimate_card);
        for class in ["ow-tone-error", "card"] {
            self.estimate_card.remove_css_class(class);
        }
        match &model.estimate {
            EstimateState::Pending => {
                self.estimate_card.add_css_class("card");
                let row = hbox(12);
                let spinner = gtk::Spinner::new();
                spinner.start();
                row.append(&spinner);
                row.append(&dim(strings::text(S::Estimating)));
                self.estimate_card.append(&row);
            }
            EstimateState::Refused(line) => {
                self.estimate_card.add_css_class(widgets::tone_class(Tone::Error));
                let row = icon_line(widgets::tone_icon(Tone::Error), strings::say(line));
                row.add_css_class("heading");
                self.estimate_card.append(&row);
                self.estimate_card.append(&text(strings::text(openworld_desktop::model::refusal_hint(&line.code))));
            }
            EstimateState::Ready(estimate) => {
                self.estimate_card.add_css_class("card");
                let time = text(strings::say(&estimate.human));
                time.add_css_class("ow-estimate-time");
                time.set_accessible_role(gtk::AccessibleRole::Heading);
                let full = estimate.detection_line.code == "detect_full";
                let summary = dim(&strings::estimate_summary(estimate.frames_analyzed, estimate.detection_long_side, full));
                self.estimate_card.append(&time);
                self.estimate_card.append(&summary);
                let notes = vbox(4);
                notes.set_margin_top(6);
                for note in &estimate.notes {
                    notes.append(&icon_line(note_icon(&note.code), strings::say(note)));
                }
                self.estimate_card.append(&notes);
                let caveat = dim(strings::say(&estimate.caveat));
                caveat.add_css_class("caption");
                self.estimate_card.append(&caveat);
            }
        }
    }

    pub fn focus(&self) {
        self.analyze.grab_focus();
    }
}

/// An icon for each estimate note, by code.
fn note_icon(code: &str) -> &'static str {
    let is = |c: openworld_core::copy::Copy| c.0 == code;
    if is(copy::CPU_NOTE) || is(copy::SUGGEST_COMPUTER) {
        "computer-symbolic"
    } else if is(copy::GPU_NOTE) {
        "applications-graphics-symbolic"
    } else if is(copy::HEAT_NOTE) || is(copy::ALREADY_WARM) {
        "weather-clear-symbolic"
    } else if is(copy::BATTERY_NOTE) {
        "battery-low-symbolic"
    } else {
        "preferences-system-time-symbolic"
    }
}
