// SPDX-License-Identifier: Apache-2.0

//! Sheets: the full disclosure, leaving for the FBI page, a candidate's frame,
//! the policies and licenses, and the confirmations for Delete and Stop.

use super::widgets::{icon_line, vbox};
use super::Ui;
use adw::prelude::*;
use gtk::gio;
use openworld_core::Line;
use openworld_desktop::calls;
use openworld_desktop::strings::{self, S};
use std::path::Path;
use std::rc::Rc;

/// A sheet with a header bar over scrolling content.
fn sheet(title: &str, content: &impl IsA<gtk::Widget>, width: i32, height: i32) -> adw::Dialog {
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(content));
    adw::Dialog::builder().title(title).child(&toolbar).content_width(width).content_height(height).build()
}

/// Every disclosure line.
pub fn read_all(ui: &Rc<Ui>, lines: &[Line]) {
    let list = vbox(10);
    for line in lines {
        list.append(&icon_line("channel-secure-symbolic", strings::say(line)));
    }
    list.set_margin_start(24);
    list.set_margin_end(24);
    list.set_margin_bottom(24);
    let scroller = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&list).vexpand(true).build();
    sheet(strings::text(S::Disclosure), &scroller, 460, 420).present(Some(&ui.window));
}

/// "You are leaving OpenWorld." Only the address the core approved opens.
pub fn leaving(ui: &Rc<Ui>, title: &Line, notes: &[Line], url: String) {
    let body: Vec<&str> = notes.iter().map(strings::say).collect();
    let dialog = adw::AlertDialog::new(Some(strings::say(title)), Some(&body.join("\n\n")));
    dialog.add_responses(&[("stay", strings::text(S::Stay)), ("open", strings::text(S::OpenFbi))]);
    dialog.set_response_appearance("open", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("stay"));
    dialog.set_close_response("stay");
    let weak = Rc::downgrade(ui);
    dialog.connect_response(None, move |_, response| {
        let Some(ui) = weak.upgrade() else { return };
        if response != "open" {
            return;
        }
        let toasts = ui.toasts.clone();
        gtk::UriLauncher::new(&url).launch(Some(&ui.window), None::<&gio::Cancellable>, move |result| {
            if result.is_err() {
                toasts.add_toast(adw::Toast::new(strings::text(S::Unopened)));
            }
        });
    });
    dialog.present(Some(&ui.window));
}

/// The frame a candidate came from.
pub fn show_frame(ui: &Rc<Ui>, frame: &Path, label: &Line) {
    let picture = gtk::Picture::for_filename(frame);
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_vexpand(true);
    picture.update_property(&[gtk::accessible::Property::Label(strings::say(label))]);
    let content = vbox(0);
    content.append(&picture);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_bottom(12);
    sheet(strings::say(label), &content, 760, 560).present(Some(&ui.window));
}

pub fn confirm_delete(ui: &Rc<Ui>) {
    let dialog = adw::AlertDialog::new(Some(strings::text(S::DeleteTitle)), Some(strings::text(S::DeleteBody)));
    dialog.add_responses(&[("cancel", strings::text(S::Cancel)), ("delete", strings::text(S::Delete))]);
    dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    let weak = Rc::downgrade(ui);
    dialog.connect_response(Some("delete"), move |_, _| {
        if let Some(ui) = weak.upgrade() {
            ui.delete_result();
        }
    });
    dialog.present(Some(&ui.window));
}

/// Back during a scan asks first. A stop is Incomplete.
pub fn confirm_stop(ui: &Rc<Ui>) {
    let dialog = adw::AlertDialog::new(Some(strings::text(S::StopTitle)), Some(strings::text(S::StopBody)));
    dialog.add_responses(&[("keep", strings::text(S::KeepScanning)), ("stop", strings::text(S::Stop))]);
    dialog.set_response_appearance("stop", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("keep"));
    dialog.set_close_response("keep");
    let weak = Rc::downgrade(ui);
    dialog.connect_response(Some("stop"), move |_, _| {
        if let Some(ui) = weak.upgrade() {
            ui.stop();
        }
    });
    dialog.present(Some(&ui.window));
}

pub fn about(ui: &Rc<Ui>) {
    let about = adw::AboutDialog::builder()
        .application_name(strings::text(S::AppName))
        .application_icon(super::APP_ID)
        .developer_name(strings::text(S::Publisher))
        .version(env!("CARGO_PKG_VERSION"))
        .comments(strings::text(S::Tagline))
        .license_type(gtk::License::Apache20)
        .website("https://github.com/isaacy13/openworld")
        .build();
    if let Ok(legal) = &ui.model.borrow().legal {
        about.set_support_url(&format!("mailto:{}", legal.contact));
    }
    about.present(Some(&ui.window));
}

/// A policy or the licenses, read from the library. The licenses run to thousands of
/// paragraphs, so they go in a text view rather than a label each.
pub fn document(ui: &Rc<Ui>, id: &str) {
    let (title, body) = match calls::legal_document(id) {
        Ok(doc) => (strings::say(&doc.title).to_string(), doc.paragraphs.join("\n\n")),
        Err(line) => (strings::text(S::AppName).to_string(), strings::say(&line).to_string()),
    };
    let view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        // License texts are wrapped by their authors for a fixed-width font.
        .monospace(id == "licenses")
        .left_margin(24)
        .right_margin(24)
        .bottom_margin(24)
        .build();
    view.buffer().set_text(&body);
    view.update_property(&[gtk::accessible::Property::Label(&title)]);
    let scroller = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&view).vexpand(true).build();
    sheet(&title, &scroller, 640, 640).present(Some(&ui.window));
}
