// SPDX-License-Identifier: Apache-2.0
package app.openworld

import app.openworld.core.Core
import app.openworld.media.MediaReader
import app.openworld.platform.Device
import app.openworld.platform.Imports
import app.openworld.platform.Paths
import app.openworld.platform.ScanMarker
import app.openworld.platform.TermsStore
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers

/** Everything the scan flow talks to. Tests pass fakes. */
class AppEnv(
    val core: Core,
    val reader: MediaReader,
    val device: Device,
    val imports: Imports,
    val marker: ScanMarker,
    val terms: TermsStore,
    val paths: Paths,
    /** Puts the bundle catalog in `paths.bundles`. Blocking. */
    val install: () -> Unit,
    /** Blocking work: core calls, decoding, file copies. */
    val io: CoroutineDispatcher = Dispatchers.IO,
    val now: () -> Long = System::currentTimeMillis,
)
