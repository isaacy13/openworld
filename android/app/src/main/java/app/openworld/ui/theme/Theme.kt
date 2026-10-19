// SPDX-License-Identifier: Apache-2.0
package app.openworld.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp

/** A foreground on its tint, from the docs/ux.md tokens. Text on each tint meets WCAG AA. */
@Immutable
data class Tone(val content: Color, val container: Color)

/** The status tokens. Material's own roles come from the wallpaper where the system offers it. */
@Immutable
data class StatusColors(
    val accent: Color,
    val candidate: Tone,
    val incomplete: Tone,
    val neutral: Tone,
    val error: Tone,
    val demo: Tone,
)

private val LightStatus = StatusColors(
    accent = Color(0xFF2F5D8C),
    candidate = Tone(Color(0xFF8F5600), Color(0xFFFFF4E0)),
    incomplete = Tone(Color(0xFFB34700), Color(0xFFFFEDE0)),
    neutral = Tone(Color(0xFF4A5560), Color(0xFFEEF1F4)),
    error = Tone(Color(0xFFB3261E), Color(0xFFFCE8E6)),
    demo = Tone(Color(0xFF6B4FA0), Color(0xFFF1ECFA)),
)

private val DarkStatus = StatusColors(
    accent = Color(0xFF8DB4E2),
    candidate = Tone(Color(0xFFFFC266), Color(0xFF3A2A10)),
    incomplete = Tone(Color(0xFFFFA066), Color(0xFF3A2210)),
    neutral = Tone(Color(0xFFC3CCD5), Color(0xFF232A31)),
    error = Tone(Color(0xFFF2B8B5), Color(0xFF3B1D1B)),
    demo = Tone(Color(0xFFC9B6F0), Color(0xFF2B2338)),
)

/** Without dynamic color, the accent leads. */
private val LightScheme = lightColorScheme(
    primary = Color(0xFF2F5D8C),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFD3E4FF),
    onPrimaryContainer = Color(0xFF001C38),
    secondary = Color(0xFF545F70),
    onSecondary = Color.White,
    secondaryContainer = Color(0xFFD8E3F8),
    onSecondaryContainer = Color(0xFF111C2B),
    surface = Color(0xFFF9F9FF),
    onSurface = Color(0xFF191C20),
    surfaceVariant = Color(0xFFDFE2EB),
    onSurfaceVariant = Color(0xFF43474E),
    outline = Color(0xFF73777F),
    outlineVariant = Color(0xFFC3C6CF),
    error = Color(0xFFB3261E),
)

private val DarkScheme = darkColorScheme(
    primary = Color(0xFF8DB4E2),
    onPrimary = Color(0xFF00315B),
    primaryContainer = Color(0xFF1B4672),
    onPrimaryContainer = Color(0xFFD3E4FF),
    secondary = Color(0xFFBCC7DB),
    onSecondary = Color(0xFF263141),
    secondaryContainer = Color(0xFF3C4758),
    onSecondaryContainer = Color(0xFFD8E3F8),
    surface = Color(0xFF111418),
    onSurface = Color(0xFFE1E2E8),
    surfaceVariant = Color(0xFF43474E),
    onSurfaceVariant = Color(0xFFC3C6CF),
    outline = Color(0xFF8D9199),
    outlineVariant = Color(0xFF43474E),
    error = Color(0xFFF2B8B5),
)

private val LocalStatus = staticCompositionLocalOf { LightStatus }

/** 12 dp cards, 20 dp sheets. */
private val OpenWorldShapes = Shapes(
    small = RoundedCornerShape(8.dp),
    medium = RoundedCornerShape(12.dp),
    large = RoundedCornerShape(16.dp),
    extraLarge = RoundedCornerShape(20.dp),
)

@Composable
fun OpenWorldTheme(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val scheme = when {
        Build.VERSION.SDK_INT >= 31 -> if (dark) dynamicDarkColorScheme(LocalContext.current) else dynamicLightColorScheme(LocalContext.current)
        dark -> DarkScheme
        else -> LightScheme
    }
    CompositionLocalProvider(LocalStatus provides if (dark) DarkStatus else LightStatus) {
        MaterialTheme(colorScheme = scheme, shapes = OpenWorldShapes, content = content)
    }
}

object OpenWorldColors {
    val status: StatusColors
        @Composable @ReadOnlyComposable get() = LocalStatus.current
}
