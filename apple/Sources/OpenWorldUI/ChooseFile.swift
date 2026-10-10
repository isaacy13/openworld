// SPDX-License-Identifier: Apache-2.0
import Foundation

/// Choose File on the phone and in the Mac window.
/// Photos and video stay listed, and every other file stays selectable.
/// An extensionless video has no movie type, so `public.data` keeps it available.
public enum ChooseFile {
    public static let typeIdentifiers = ["public.image", "public.movie", "public.data"]
    /// The share sheet accepts the same files as Choose File, including an extensionless video.
    public static let shareTypeIdentifiers = typeIdentifiers
}
