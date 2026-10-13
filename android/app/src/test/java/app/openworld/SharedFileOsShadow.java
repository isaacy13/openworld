// SPDX-License-Identifier: Apache-2.0
package app.openworld;

import android.system.StructStat;
import android.system.StructTimespec;
import java.io.File;
import java.io.FileDescriptor;
import org.robolectric.annotation.Implementation;
import org.robolectric.annotation.Implements;

/**
 * Robolectric's {@code Os.fstat} returns a zero stat for a content file.
 * The phone reads seconds from the real descriptor; this shadow supplies those seconds.
 */
@Implements(android.system.Os.class)
public class SharedFileOsShadow {
    public static volatile File opened;

    @Implementation
    protected static StructStat fstat(FileDescriptor fd) {
        File file = opened;
        long seconds = file == null ? 0L : file.lastModified() / 1000L;
        long size = file == null ? 0L : file.length();
        return new StructStat(
            0L,
            0L,
            0,
            0L,
            0,
            0,
            0L,
            size,
            new StructTimespec(0L, 0L),
            new StructTimespec(seconds, 0L),
            new StructTimespec(0L, 0L),
            0L,
            0L
        );
    }
}
