// SPDX-License-Identifier: Apache-2.0

//! Small building blocks shared by the screens.

use adw::prelude::*;
use gtk::{gdk, glib, pango};
use openworld_desktop::calls::Thumb;
use openworld_desktop::model::Tone;
use openworld_desktop::strings::{self, S};
use std::path::Path;

/// A label that wraps at words and never ends in "…".
pub fn text(content: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(content)
        .wrap(true)
        .wrap_mode(pango::WrapMode::WordChar)
        .xalign(0.0)
        .halign(gtk::Align::Fill)
        .selectable(false)
        .build()
}

/// A file name or title: wraps anywhere it must, but never adds a hyphen that is not in the name.
pub fn name(content: &str) -> gtk::Label {
    let label = heading(content);
    let attrs = pango::AttrList::new();
    attrs.insert(pango::AttrInt::new_insert_hyphens(false));
    label.set_attributes(Some(&attrs));
    label
}

/// A tinted inline row: an icon and a line, in a tone's colors.
pub fn tinted(icon: &str, content: &str, tone: Tone) -> gtk::Box {
    let row = icon_line(icon, content);
    row.add_css_class("ow-inline");
    row.add_css_class(tone_class(tone));
    row
}

pub fn dim(content: &str) -> gtk::Label {
    let label = text(content);
    label.add_css_class("dim-label");
    label
}

/// A section heading, announced as a heading.
pub fn heading(content: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(content)
        .wrap(true)
        .wrap_mode(pango::WrapMode::WordChar)
        .xalign(0.0)
        .accessible_role(gtk::AccessibleRole::Heading)
        .build();
    label.add_css_class("heading");
    label
}

pub fn vbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, spacing)
}

pub fn hbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Horizontal, spacing)
}

/// A rounded card with padding.
pub fn card(spacing: i32) -> gtk::Box {
    let card = vbox(spacing);
    card.add_css_class("card");
    card.add_css_class("ow-card");
    card
}

pub fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

/// An icon beside a wrapped line.
pub fn icon_line(icon: &str, content: &str) -> gtk::Box {
    let row = hbox(10);
    row.add_css_class("ow-line");
    let image = gtk::Image::from_icon_name(icon);
    image.set_valign(gtk::Align::Start);
    image.set_margin_top(2);
    image.set_accessible_role(gtk::AccessibleRole::Presentation);
    let label = text(content);
    label.set_hexpand(true);
    row.append(&image);
    row.append(&label);
    row
}

pub fn tone_class(tone: Tone) -> &'static str {
    match tone {
        Tone::Candidate => "ow-tone-candidate",
        Tone::Neutral => "ow-tone-neutral",
        Tone::Incomplete => "ow-tone-incomplete",
        Tone::Error => "ow-tone-error",
    }
}

/// Status is never color alone: each tone has its icon.
pub fn tone_icon(tone: Tone) -> &'static str {
    match tone {
        Tone::Candidate => "view-reveal-symbolic",
        Tone::Neutral => "dialog-information-symbolic",
        Tone::Incomplete => "dialog-warning-symbolic",
        Tone::Error => "dialog-error-symbolic",
    }
}

/// Fill a status pill: an icon and words, colored by tone.
pub fn set_chip(chip: &gtk::Box, label: S, tone: Tone) {
    clear(chip);
    for class in ["ow-tone-candidate", "ow-tone-neutral", "ow-tone-incomplete", "ow-tone-error"] {
        chip.remove_css_class(class);
    }
    chip.add_css_class(tone_class(tone));
    let icon = gtk::Image::from_icon_name(if label == S::ChipLooking { "image-loading-symbolic" } else { tone_icon(tone) });
    icon.set_pixel_size(12);
    chip.append(&icon);
    chip.append(&gtk::Label::new(Some(strings::text(label))));
}

/// A picture from a file, shown in a `size` square.
pub fn picture(path: Option<&Path>, size: i32, fallback_icon: &str) -> gtk::Widget {
    let image = match path.filter(|p| p.is_file()).and_then(|p| gdk::Texture::from_filename(p).ok()) {
        Some(texture) => gtk::Image::from_paintable(Some(&texture)),
        None => {
            let image = gtk::Image::from_icon_name(fallback_icon);
            image.add_css_class("dim-label");
            image
        }
    };
    square(image, size)
}

/// A thumbnail the core decoded, already square.
pub fn thumb_picture(thumb: &Thumb, size: i32) -> gtk::Widget {
    let bytes = glib::Bytes::from(&thumb.rgb[..]);
    let texture =
        gdk::MemoryTexture::new(thumb.width as i32, thumb.height as i32, gdk::MemoryFormat::R8g8b8, &bytes, thumb.width as usize * 3);
    square(gtk::Image::from_paintable(Some(&texture)), size)
}

/// An image is drawn at exactly `size`, whatever its pixels.
fn square(image: gtk::Image, size: i32) -> gtk::Widget {
    image.set_pixel_size(size);
    image.set_halign(gtk::Align::Center);
    image.set_valign(gtk::Align::Start);
    image.set_overflow(gtk::Overflow::Hidden);
    image.add_css_class("ow-picture");
    image.upcast()
}

/// A wide pill button.
pub fn pill(label: &str, class: Option<&str>) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.add_css_class("pill");
    if let Some(class) = class {
        button.add_css_class(class);
    }
    button.set_halign(gtk::Align::Center);
    button
}

/// Page content: scrolls, centered, 24 px gutters (16 px when `narrow` applies).
pub fn scroller(child: &impl IsA<gtk::Widget>, width: i32, narrow: &adw::Breakpoint) -> gtk::ScrolledWindow {
    let clamp = adw::Clamp::builder().maximum_size(width).tightening_threshold(width * 3 / 4).child(child).build();
    for side in ["margin-top", "margin-bottom", "margin-start", "margin-end"] {
        clamp.set_property(side, 24);
        narrow.add_setter(&clamp, side, Some(&16.to_value()));
    }
    gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&clamp).vexpand(true).build()
}

/// A page with a header bar over scrolling content.
pub fn page(title: &str, tag: &str, header: &adw::HeaderBar, content: &impl IsA<gtk::Widget>) -> (adw::NavigationPage, adw::ToolbarView) {
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(header);
    toolbar.set_content(Some(content));
    let page = adw::NavigationPage::builder().title(title).tag(tag).child(&toolbar).build();
    (page, toolbar)
}

/// A face tile: crop, status chip, and a caption under it.
pub struct FaceTile {
    pub root: gtk::Box,
    pub chip: gtk::Box,
    pub caption: gtk::Label,
    pub picture: gtk::Box,
}

impl FaceTile {
    pub fn new(size: i32) -> Self {
        // A group, so a screen reader reads its label: "Face from frame 12. Possible candidate."
        let root =
            gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).accessible_role(gtk::AccessibleRole::Group).build();
        root.add_css_class("ow-face");
        root.set_size_request(size + 16, -1);
        let picture = vbox(0);
        let chip = hbox(4);
        chip.add_css_class("ow-chip");
        chip.set_halign(gtk::Align::Start);
        let caption = dim("");
        caption.add_css_class("caption");
        caption.set_max_width_chars(16);
        root.append(&picture);
        root.append(&chip);
        root.append(&caption);
        Self { root, chip, caption, picture }
    }

    pub fn set_crop(&self, path: Option<&Path>, size: i32) {
        clear(&self.picture);
        self.picture.append(&picture(path, size, "avatar-default-symbolic"));
    }
}
