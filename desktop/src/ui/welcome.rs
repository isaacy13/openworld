// SPDX-License-Identifier: Apache-2.0

//! First run: what OpenWorld is and is not, the policies, and one Continue.
//! It stands in for the whole window until the core's terms version is accepted.

use super::widgets::{self, dim, icon_line, tinted, vbox};
use super::{dialogs, Ui};
use adw::prelude::*;
use openworld_desktop::model::Tone;
use openworld_desktop::strings::{self, S};
use std::rc::Rc;

/// The documents linked on this screen. About links every one.
const LINKED: [&str; 3] = ["terms", "privacy", "biometric"];

pub fn build(ui: &Rc<Ui>, narrow: &adw::Breakpoint) -> adw::ToolbarView {
    let content = vbox(20);
    let title =
        gtk::Label::builder().label(strings::text(S::Welcome)).wrap(true).accessible_role(gtk::AccessibleRole::Heading).build();
    title.add_css_class("ow-title");
    content.append(&title);

    let model = ui.model.borrow();
    if let Some(changed) = model.terms_changed() {
        content.append(&tinted("dialog-information-symbolic", strings::say(changed), Tone::Neutral));
    }
    let continue_button = widgets::pill(strings::text(S::Continue), Some("suggested-action"));
    match &model.legal {
        Ok(legal) => {
            let list = vbox(10);
            for line in &legal.first_run {
                list.append(&icon_line("channel-secure-symbolic", strings::say(line)));
            }
            content.append(&list);
            let links = gtk::FlowBox::builder().selection_mode(gtk::SelectionMode::None).max_children_per_line(3).build();
            for row in legal.documents.iter().filter(|row| LINKED.contains(&row.id.as_str())) {
                let link = gtk::Button::with_label(strings::say(&row.title));
                link.add_css_class("flat");
                let (weak, id) = (Rc::downgrade(ui), row.id.clone());
                link.connect_clicked(move |_| {
                    if let Some(ui) = weak.upgrade() {
                        dialogs::document(&ui, &id);
                    }
                });
                links.append(&link);
            }
            content.append(&links);
            content.append(&dim(strings::say(&legal.accept)));
        }
        // Without the terms there is nothing to accept, so Continue stays off.
        Err(line) => {
            content.append(&tinted("dialog-error-symbolic", strings::say(line), Tone::Error));
            continue_button.set_sensitive(false);
        }
    }
    drop(model);
    let weak = Rc::downgrade(ui);
    continue_button.connect_clicked(move |_| {
        if let Some(ui) = weak.upgrade() {
            ui.accept_terms();
        }
    });
    content.append(&continue_button);
    content.set_valign(gtk::Align::Center);

    let header = adw::HeaderBar::new();
    header.set_show_title(false);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&widgets::scroller(&content, 640, narrow)));
    continue_button.grab_focus();
    toolbar
}
