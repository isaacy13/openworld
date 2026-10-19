// SPDX-License-Identifier: Apache-2.0

//! A dev build only (`--features shots`): drive every screen with the demo
//! posters and synthetic files, and save each as a PNG for review.
//! It calls the same methods the buttons call. A step that does not reach its
//! state fails the run, so this doubles as an end-to-end check.

use crate::ui::{dialogs, Ui};
use adw::prelude::*;
use gtk::{gio, glib, graphene};
use openworld_desktop::model::{EstimateState, Model, PackState, Phase};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

pub fn run(ui: Rc<Ui>, dir: PathBuf, status: Rc<Cell<i32>>) {
    ui.window.set_default_size(900, 1500);
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(false);
    }
    glib::spawn_future_local(async move {
        let outcome = script(&ui, &dir).await;
        let closing = match outcome {
            Ok(count) => {
                println!("saved {count} screenshots to {}", dir.display());
                close_during_scan(&ui).await
            }
            Err(why) => Err(why),
        };
        if let Err(why) = closing {
            eprintln!("screenshots failed: {why}");
            status.set(1);
            ui.model.borrow_mut().close();
            ui.app.quit();
        }
    });
}

/// Last step: close the window while a scan runs. The app stops it, removes its
/// folder, and quits; the caller checks the printed folder is gone.
async fn close_during_scan(ui: &Rc<Ui>) -> Result<(), String> {
    let video = glib::user_data_dir().join("shots-input/demo-clip.mkv");
    open(ui, &video).await?;
    ui.analyze();
    until(ui, "a face", |m| matches!(&m.phase, Phase::Scanning(s) if !s.faces.is_empty())).await?;
    if let Phase::Scanning(scan) = &ui.model.borrow().phase {
        println!("closing during a scan: {}", scan.out.display());
    }
    ui.window.close();
    Ok(())
}

async fn wait(ms: u64) {
    glib::timeout_future(Duration::from_millis(ms)).await;
}

async fn until(ui: &Rc<Ui>, what: &str, done: impl Fn(&Model) -> bool) -> Result<(), String> {
    for _ in 0..1200 {
        if done(&ui.model.borrow()) {
            return Ok(());
        }
        wait(50).await;
    }
    Err(format!("timed out waiting for {what}"))
}

struct Shots<'a> {
    ui: &'a Rc<Ui>,
    dir: &'a Path,
    count: usize,
    suffix: &'static str,
}

impl Shots<'_> {
    /// Let layout settle, then render the window, with any sheet over it, to a PNG.
    async fn take(&mut self, name: &str) -> Result<(), String> {
        wait(450).await;
        let window = &self.ui.window;
        // A frame can be skipped while messages arrive; try a few times.
        for _ in 0..20 {
            let (w, h) = (window.width(), window.height());
            let paintable = gtk::WidgetPaintable::new(Some(window));
            let snapshot = gtk::Snapshot::new();
            paintable.snapshot(&snapshot, w as f64, h as f64);
            if let (Some(node), Some(renderer)) = (snapshot.to_node(), window.renderer()) {
                let texture = renderer.render_texture(&node, Some(&graphene::Rect::new(0.0, 0.0, w as f32, h as f32)));
                self.count += 1;
                let path = self.dir.join(format!("{:02}-{name}{}.png", self.count, self.suffix));
                return texture.save_to_png(&path).map_err(|e| e.to_string());
            }
            wait(100).await;
        }
        Err(format!("nothing was drawn for {name} (mapped: {})", window.is_mapped()))
    }
}

fn close_sheet(ui: &Rc<Ui>) {
    if let Some(dialog) = ui.window.visible_dialog() {
        dialog.force_close();
    }
}

/// The first widget of type `T` under `root`, depth first.
fn find<T: IsA<gtk::Widget>>(root: &gtk::Widget) -> Option<T> {
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Ok(found) = widget.clone().downcast::<T>() {
            if widget.is_mapped() {
                return Some(found);
            }
        }
        if let Some(found) = find::<T>(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

fn expand_visible(ui: &Rc<Ui>) {
    if let Some(row) = find::<adw::ExpanderRow>(ui.window.upcast_ref()) {
        row.set_expanded(true);
    }
}

fn result_ready(model: &Model) -> bool {
    matches!(model.phase, Phase::Result(_))
}

fn review_ready(model: &Model) -> bool {
    matches!(model.phase, Phase::Review)
        && !matches!(model.estimate, EstimateState::Pending)
        && model.file.as_ref().is_some_and(|f| f.facts.is_some())
}

async fn open(ui: &Rc<Ui>, path: &Path) -> Result<(), String> {
    ui.reset();
    ui.open_file(gio::File::for_path(path));
    until(ui, "the review page", review_ready).await
}

async fn candidate_flow(ui: &Rc<Ui>, shots: &mut Shots<'_>, still: &Path) -> Result<(), String> {
    ui.reset();
    ui.model.borrow_mut().set_classes(true, true);
    shots.take("start").await?;
    open(ui, still).await?;
    shots.take("review").await?;
    ui.analyze();
    until(ui, "a result", result_ready).await?;
    shots.take("result-candidate").await
}

async fn script(ui: &Rc<Ui>, dir: &Path) -> Result<usize, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let work = glib::user_data_dir().join("shots-input");
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let still = work.join("IMG_20261019_101112_demo-scene_with_a_long_name_that_has_to_wrap.png");
    openworld_core::fixture::demo_scene().image.save(&still).map_err(|e| e.to_string())?;
    let mut shots = Shots { ui, dir, count: 0, suffix: "" };

    until(ui, "the poster check", |m| !matches!(m.pack, PackState::Checking)).await?;
    ui.window.present();
    shots.take("start-no-posters").await?;
    ui.install_demo();
    until(ui, "the demo pack", |m| matches!(m.pack, PackState::Ready(_))).await?;
    shots.take("start").await?;

    open(ui, &still).await?;
    shots.take("review").await?;
    expand_visible(ui);
    let job = ui.model.borrow_mut().set_coverage(openworld_core::Coverage::Measured);
    job.into_iter().for_each(|job| ui.spawn(job));
    until(ui, "the measured estimate", |m| matches!(m.estimate, EstimateState::Ready(_))).await?;
    ui.render();
    shots.take("review-options-measured").await?;
    let job = ui.model.borrow_mut().set_coverage(openworld_core::Coverage::Complete);
    job.into_iter().for_each(|job| ui.spawn(job));
    until(ui, "the estimate", |m| matches!(m.estimate, EstimateState::Ready(_))).await?;
    ui.model.borrow_mut().set_classes(false, false);
    ui.render();
    shots.take("review-no-class").await?;
    ui.model.borrow_mut().set_classes(true, true);
    ui.render();
    dialogs::read_all(ui, &openworld_core::report::disclosure(true));
    shots.take("read-all").await?;
    close_sheet(ui);

    ui.analyze();
    until(ui, "a result", result_ready).await?;
    shots.take("result-candidate").await?;
    expand_visible(ui);
    shots.take("result-details").await?;
    let (url, notes, frame, label) = match &ui.model.borrow().phase {
        Phase::Result(done) => {
            let card = done.report.candidates.first().ok_or("no candidate card")?;
            (
                card.poster.fbi_url.clone().ok_or("no FBI page")?,
                card.leave_notes.clone(),
                done.out.join(card.frame.clone().unwrap_or_default()),
                card.frame_label.clone(),
            )
        }
        _ => return Err("left the result".into()),
    };
    ui.leave(&url, &notes);
    shots.take("leaving").await?;
    close_sheet(ui);
    dialogs::show_frame(ui, &frame, &label);
    shots.take("show-frame").await?;
    close_sheet(ui);
    dialogs::confirm_delete(ui);
    shots.take("delete-confirm").await?;
    close_sheet(ui);

    open(ui, &still).await?;
    ui.model.borrow_mut().set_classes(false, true);
    ui.analyze();
    until(ui, "a result", result_ready).await?;
    shots.take("result-no-candidate").await?;

    // A long lossless video, so the scan is still running for the picture.
    let video = work.join("demo-clip.mkv");
    if !video.is_file() {
        let made = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-loop", "1", "-i"])
            .arg(&still)
            .args(["-t", "120", "-r", "30", "-c:v", "ffv1", "-pix_fmt", "bgr0"])
            .arg(&video)
            .status()
            .map_err(|e| e.to_string())?;
        if !made.success() {
            return Err("ffmpeg could not make the demo video".into());
        }
    }
    open(ui, &video).await?;
    shots.take("review-video").await?;
    ui.analyze();
    until(ui, "a face on the scan screen", |m| matches!(&m.phase, Phase::Scanning(s) if !s.faces.is_empty() && s.analyzed > 0)).await?;
    wait(800).await;
    ui.render();
    shots.take("scanning").await?;
    dialogs::confirm_stop(ui);
    shots.take("stop-confirm").await?;
    close_sheet(ui);
    ui.stop();
    until(ui, "the stopped result", result_ready).await?;
    shots.take("result-incomplete").await?;

    let junk = work.join("not-a-picture.png");
    std::fs::write(&junk, b"plain text").map_err(|e| e.to_string())?;
    open(ui, &junk).await?;
    shots.take("review-refused").await?;

    // No poster pack: the scan refuses with its sentence and what to do.
    let posters = ui.model.borrow().places.posters.clone();
    let aside = posters.with_extension("aside");
    std::fs::rename(&posters, &aside).map_err(|e| e.to_string())?;
    open(ui, &still).await?;
    ui.analyze();
    until(ui, "a refusal", result_ready).await?;
    shots.take("result-refused").await?;
    std::fs::rename(&aside, &posters).map_err(|e| e.to_string())?;

    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    shots.suffix = "-dark";
    candidate_flow(ui, &mut shots, &still).await?;
    open(ui, &video).await?;
    ui.analyze();
    until(ui, "a face", |m| matches!(&m.phase, Phase::Scanning(s) if !s.faces.is_empty())).await?;
    ui.stop();
    until(ui, "the stopped result", result_ready).await?;
    shots.take("result-incomplete").await?;
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);

    // Phone width: 16 px gutters are the phone's; this checks the desktop layout still fits.
    ui.window.set_visible(false);
    ui.window.set_default_size(380, 1500);
    ui.window.set_visible(true);
    shots.suffix = "-narrow";
    candidate_flow(ui, &mut shots, &still).await?;
    Ok(shots.count)
}
