// SPDX-License-Identifier: Apache-2.0

//! Start: the title, the drop card, the trust chips, and the poster pack.

use super::widgets::{self, dim, hbox, text, vbox};
use super::{dialogs, Ui};
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use openworld_core::copy;
use openworld_core::Line;
use openworld_desktop::model::PackState;
use openworld_desktop::strings::{self, S};
use std::rc::Rc;
use std::time::SystemTime;

pub struct Start {
    pub page: adw::NavigationPage,
    content: gtk::Box,
    drop_card: gtk::Box,
    choose: gtk::Button,
    badge: gtk::Label,
    pack_line: gtk::Label,
    demo_note: gtk::Label,
    install: gtk::Button,
    installing: gtk::Spinner,
}

impl Start {
    pub fn build(narrow: &adw::Breakpoint) -> Self {
        let content = vbox(28);

        let title_box = vbox(10);
        let title = gtk::Label::builder().label(strings::text(S::AppName)).accessible_role(gtk::AccessibleRole::Heading).build();
        title.add_css_class("ow-title");
        let tagline = text(strings::text(S::Tagline));
        tagline.set_xalign(0.5);
        tagline.set_justify(gtk::Justification::Center);
        tagline.add_css_class("ow-tagline");
        tagline.set_max_width_chars(48);
        tagline.set_halign(gtk::Align::Center);
        title_box.append(&title);
        title_box.append(&tagline);
        title_box.set_margin_top(12);

        let drop_card = vbox(14);
        drop_card.add_css_class("ow-drop");
        let icon = gtk::Image::from_icon_name("folder-pictures-symbolic");
        icon.set_pixel_size(56);
        icon.add_css_class("dim-label");
        icon.set_accessible_role(gtk::AccessibleRole::Presentation);
        let choose = widgets::pill(strings::text(S::ChooseFile), Some("suggested-action"));
        choose.set_tooltip_text(Some("Ctrl+O"));
        let or_drop = dim(strings::text(S::OrDropHere));
        or_drop.set_xalign(0.5);
        drop_card.append(&icon);
        drop_card.append(&choose);
        drop_card.append(&or_drop);

        let chips = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .max_children_per_line(3)
            .min_children_per_line(1)
            .column_spacing(8)
            .row_spacing(8)
            .homogeneous(true)
            .build();
        for (icon, label) in [
            ("channel-secure-symbolic", S::TrustOnDevice),
            ("network-offline-symbolic", S::TrustNothingUploaded),
            ("camera-disabled-symbolic", S::TrustNoCamera),
        ] {
            let chip = hbox(8);
            chip.add_css_class("ow-trust");
            chip.append(&gtk::Image::from_icon_name(icon));
            chip.append(&gtk::Label::new(Some(strings::text(label))));
            chip.set_halign(gtk::Align::Center);
            chips.append(&chip);
        }
        chips.set_focusable(false);

        let footer = vbox(8);
        footer.set_halign(gtk::Align::Center);
        let pack_row = hbox(8);
        pack_row.set_halign(gtk::Align::Center);
        let badge = gtk::Label::new(Some(strings::text(S::DemoBadge)));
        badge.add_css_class("ow-badge");
        badge.add_css_class("ow-demo");
        badge.set_valign(gtk::Align::Center);
        let pack_line = dim("");
        pack_line.set_xalign(0.5);
        pack_row.append(&badge);
        pack_row.append(&pack_line);
        let demo_note = dim("");
        demo_note.set_xalign(0.5);
        demo_note.add_css_class("caption");
        let install_row = hbox(8);
        install_row.set_halign(gtk::Align::Center);
        let install = gtk::Button::with_label(strings::text(S::InstallDemo));
        install.add_css_class("pill");
        let installing = gtk::Spinner::new();
        install_row.append(&install);
        install_row.append(&installing);
        let kept = dim(strings::text(S::ResultsKept));
        kept.set_xalign(0.5);
        kept.add_css_class("caption");
        footer.append(&pack_row);
        footer.append(&demo_note);
        footer.append(&install_row);
        footer.append(&kept);

        content.append(&title_box);
        content.append(&drop_card);
        content.append(&chips);
        content.append(&footer);

        let header = adw::HeaderBar::new();
        header.set_show_title(false);
        let menu = gio::Menu::new();
        menu.append(Some(strings::text(S::About)), Some("win.about"));
        let about = gtk::MenuButton::builder().icon_name("open-menu-symbolic").menu_model(&menu).primary(true).build();
        about.set_tooltip_text(Some(strings::text(S::MainMenu)));
        header.pack_end(&about);
        content.set_valign(gtk::Align::Center);
        let scroller = widgets::scroller(&content, 640, narrow);
        let (page, _) = widgets::page(strings::text(S::AppName), "start", &header, &scroller);
        Self { page, content, drop_card, choose, badge, pack_line, demo_note, install, installing }
    }

    pub fn wire(&self, ui: &Rc<Ui>) {
        let weak = Rc::downgrade(ui);
        self.choose.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.pick_file();
            }
        });
        let weak = Rc::downgrade(ui);
        self.install.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.install_demo();
            }
        });
        let about = gio::SimpleAction::new("about", None);
        let weak = Rc::downgrade(ui);
        about.connect_activate(move |_, _| {
            if let Some(ui) = weak.upgrade() {
                dialogs::about(&ui);
            }
        });
        ui.window.add_action(&about);

        // A file dropped anywhere on the start page opens.
        let target = gtk::DropTarget::new(gio::File::static_type(), gdk::DragAction::COPY);
        let card = self.drop_card.clone();
        target.connect_enter(move |_, _, _| {
            card.add_css_class("ow-drop-hover");
            gdk::DragAction::COPY
        });
        let card = self.drop_card.clone();
        target.connect_leave(move |_| card.remove_css_class("ow-drop-hover"));
        let card = self.drop_card.clone();
        let weak = Rc::downgrade(ui);
        target.connect_drop(move |_, value: &glib::Value, _, _| {
            card.remove_css_class("ow-drop-hover");
            let (Some(ui), Ok(file)) = (weak.upgrade(), value.get::<gio::File>()) else { return false };
            ui.open_file(file);
            true
        });
        self.content.add_controller(target);
    }

    pub fn update(&self, ui: &Rc<Ui>) {
        let model = ui.model.borrow();
        let (missing, wanted) = model.class_counts();
        self.installing.set_spinning(matches!(model.pack, PackState::Installing));
        self.installing.set_visible(matches!(model.pack, PackState::Installing));
        match &model.pack {
            PackState::Ready(info) => {
                self.badge.set_visible(info.demo);
                self.pack_line.set_text(&strings::pack_footer(info.demo, missing + wanted, model.pack_days(SystemTime::now())));
                self.demo_note.set_visible(info.demo);
                self.demo_note.set_text(strings::say(&Line::of(copy::FIXTURE_MARKERS)));
            }
            PackState::Refused(line) if line.is(copy::R_PACK_MISSING) => {
                self.badge.set_visible(false);
                self.pack_line.set_text(strings::text(S::NoPosters));
                self.demo_note.set_visible(false);
            }
            PackState::Refused(line) => {
                self.badge.set_visible(false);
                self.pack_line.set_text(strings::say(line));
                self.demo_note.set_visible(false);
            }
            PackState::Checking | PackState::Installing => {
                self.badge.set_visible(false);
                self.pack_line.set_text("");
                self.demo_note.set_visible(false);
            }
        }
        self.install.set_visible(model.offers_demo() || matches!(model.pack, PackState::Installing));
        self.install.set_sensitive(!matches!(model.pack, PackState::Installing));
    }

    pub fn focus(&self) {
        self.choose.grab_focus();
    }
}
