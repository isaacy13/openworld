// SPDX-License-Identifier: Apache-2.0

//! The parts of the desktop app that do not need GTK: typed calls into the
//! scan library, where files live, the shell's words, and the window's state.
//! The window itself is in `main.rs` and `ui/`.

pub mod calls;
pub mod model;
pub mod places;
pub mod strings;
