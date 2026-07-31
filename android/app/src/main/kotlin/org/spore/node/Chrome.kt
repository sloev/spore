package org.spore.node

import android.provider.Settings
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The Neo-Tokyo Tactical Wasteland chrome, in Compose.
 *
 * [docs/VISUALDESIGN.md](../../../../../../../docs/VISUALDESIGN.md) is normative and
 * this file is its Android consumer. Colour was already implemented; §3's *shapes*
 * — the ammo crate, the Toughbook input, the radio switch, the segmented LED —
 * were not, and the spec's own status table said so. This is that half.
 *
 * Two rules from the spec are load-bearing here and easy to undo by accident:
 *
 *  * **Hard edges.** ≤2 dp corners and a 4 dp *hard offset* shadow with no blur.
 *    Compose's `Modifier.shadow` draws a blurred elevation shadow, which is the
 *    Material look this language exists to avoid — so the shadow is drawn by hand
 *    in [crate] and [radioFace]. Reach for `shadow()` here and the crate stops
 *    being a crate.
 *  * **Never pink on olive.** 2.32:1, the one pairing §1 forbids outright. It is
 *    also the pairing the palette invites, so [StickerBadge] takes its own
 *    background and the pink variants sit on void.
 */

// -- palette ------------------------------------------------------------------

/**
 * The §1 tokens. Ratios in comments are measured, not estimated — recompute them
 * if a value ever changes, because §0 constraint 3 is the reason this palette is
 * usable at all.
 */
internal object Palette {
    val Void = Color(0xFF0A0A0C) // CRT Black — page base
    val Asphalt = Color(0xFF1A1C20) // Worn Asphalt — panels, crates
    val Kevlar = Color(0xFF4B5320) // Kevlar Olive — crate fills, inert chrome
    val Amber = Color(0xFFFFB000) // 10.80:1 on Void, 9.31 on Asphalt, 4.48 on Kevlar (large only)
    val Phosphor = Color(0xFF39FF14) // 14.59 / 12.58 / 6.06
    val Pink = Color(0xFFFF2A85) // 5.58 / 4.81 / 2.32 ← never on Kevlar
    val Cyan = Color(0xFF00FFFF) // 15.78 / 13.61 / 6.55
    val Edge = Color(0xFF2A2F1C) // machined metal — borders
    val Dim = Color(0xFF8A7A4A) // de-emphasised text — 4.68:1 on Void

    // Field Notes (light) — each re-checked to clear 4.5:1 on paper
    val Paper = Color(0xFFF4F1E8)
    val PaperInk = Color(0xFF1A1C20)
    val AmberDark = Color(0xFF8A5F00) // 5.00:1
    val PhosphorDark = Color(0xFF1F7A0C) // 4.83:1
    val PinkDark = Color(0xFFC2185B) // 5.20:1
    val CyanDark = Color(0xFF00707A) // 5.17:1
    val PaperEdge = Color(0xFFD8D2C0)
}

/** Hard-edged everywhere: §3 allows 2 dp and no more. */
internal val CrateShape = RoundedCornerShape(2.dp)

// The bunker is dark, so dark is the real theme; Field Notes is the printed-manual
// voice rather than a wash of the same one.
internal val SporeLightColors = lightColorScheme(
    primary = Palette.PinkDark,
    secondary = Palette.CyanDark,
    tertiary = Palette.PhosphorDark,
    background = Palette.Paper,
    surface = Palette.Paper,
    surfaceVariant = Color(0xFFFFFFFF),
    onBackground = Palette.PaperInk,
    onSurface = Palette.PaperInk,
    outline = Palette.PaperEdge,
    error = Palette.PinkDark,
)

internal val SporeDarkColors = darkColorScheme(
    primary = Palette.Pink,
    secondary = Palette.Cyan,
    tertiary = Palette.Phosphor,
    background = Palette.Void,
    surface = Palette.Void,
    surfaceVariant = Palette.Asphalt,
    onBackground = Palette.Amber,
    onSurface = Palette.Amber,
    onPrimary = Palette.Void,
    outline = Palette.Edge,
    // The palette has no red, so error shares the accent hue. Never signal a
    // failure by colour alone here — pair it with an icon and words (§1).
    error = Palette.Pink,
)

/**
 * §2: everything is monospace, because this is a terminal. Applied as a whole
 * [Typography] so a plain `Text` is right by default and nobody has to remember.
 * No downloaded font anywhere — constraint 1 forbids it and CI greps for it.
 */
internal val SporeTypography: Typography = Typography().let { d ->
    fun TextStyle.mono() = copy(fontFamily = FontFamily.Monospace)
    Typography(
        displayLarge = d.displayLarge.mono(), displayMedium = d.displayMedium.mono(),
        displaySmall = d.displaySmall.mono(), headlineLarge = d.headlineLarge.mono(),
        headlineMedium = d.headlineMedium.mono(), headlineSmall = d.headlineSmall.mono(),
        titleLarge = d.titleLarge.mono(), titleMedium = d.titleMedium.mono(),
        titleSmall = d.titleSmall.mono(), bodyLarge = d.bodyLarge.mono(),
        bodyMedium = d.bodyMedium.mono(), bodySmall = d.bodySmall.mono(),
        labelLarge = d.labelLarge.mono(), labelMedium = d.labelMedium.mono(),
        labelSmall = d.labelSmall.mono(),
    )
}

// -- reduced motion -----------------------------------------------------------

/**
 * Android's equivalent of `prefers-reduced-motion: reduce`.
 *
 * There is no Compose API for this. The platform signal is the animator duration
 * scale, which Developer Options and the accessibility "remove animations" toggle
 * both drive to 0. Read once per composition rather than per frame — it changes
 * about as often as a device reboot, and polling it in a draw scope would be
 * absurd.
 *
 * Constraint 2 says the interface must be *completely* static under this, not
 * merely calmer: a CRT flicker is a photosensitivity trigger.
 */
@Composable
internal fun reducedMotion(): Boolean {
    val ctx = LocalContext.current
    return remember(ctx) {
        runCatching {
            Settings.Global.getFloat(ctx.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
        }.getOrDefault(false)
    }
}

/**
 * §4 scanlines: a 2 dp period, ≤6% black, drawn over the content and never
 * intercepting touches (drawing is not an input target, so this is free here).
 * Static by construction — no animation to disable — but still dropped entirely
 * under reduced motion, since a fixed line grid is itself a shimmer source on an
 * LCD and the spec asks for *completely* static.
 */
internal fun Modifier.scanlines(enabled: Boolean): Modifier =
    if (!enabled) this else drawWithContent {
        drawContent()
        val period = 2.dp.toPx()
        var y = 0f
        while (y < size.height) {
            drawRect(
                color = Color.Black.copy(alpha = 0.06f),
                topLeft = Offset(0f, y),
                size = Size(size.width, period / 2f),
            )
            y += period
        }
    }

// -- the ammo crate -----------------------------------------------------------

/**
 * §3's container: `--panel` fill, 2 px `--edge` border, 2 px hard offset shadow,
 * no blur, no rounding beyond 2 px.
 *
 * The shadow is painted into reserved padding rather than outside the layout
 * bounds, so a crate never bleeds over its neighbour in a `Column`. That is why
 * the padding comes first: it shrinks the draw area to the visible face and
 * leaves exactly [depth] on the shadow side.
 */
internal fun Modifier.crate(
    fill: Color = Palette.Asphalt,
    edge: Color = Palette.Edge,
    depth: Dp = 4.dp,
): Modifier = this
    .padding(end = depth, bottom = depth)
    .drawBehind {
        drawRect(
            color = Color.Black.copy(alpha = 0.6f),
            topLeft = Offset(depth.toPx(), depth.toPx()),
            size = size,
        )
    }
    .background(fill, CrateShape)
    .border(2.dp, edge, CrateShape)

/** A crate as a container. [edge] carries meaning — pink for danger, cyan for focus. */
@Composable
internal fun Crate(
    modifier: Modifier = Modifier,
    fill: Color = Palette.Asphalt,
    edge: Color = Palette.Edge,
    content: @Composable () -> Unit,
) {
    Box(modifier.crate(fill, edge).padding(12.dp)) { content() }
}

// -- display heading ----------------------------------------------------------

/**
 * §2 headers: uppercase, heavy, tight tracking, amber, with the 2 px CRT bloom.
 *
 * The spec names Impact/Haettenschweiler; constraint 1 forbids downloading them
 * and neither ships on Android. `FontWeight.Black` on the system sans is the
 * closest honest stand-in — condensed-ness is the part we lose, and pretending
 * otherwise by shipping a webfont would break the standalone's zero-request rule.
 *
 * The bloom is a real 2 px blur via [Shadow], dropped under reduced motion where
 * §2 says it reads as blur rather than glow.
 */
@Composable
internal fun DisplayHeading(
    text: String,
    modifier: Modifier = Modifier,
    color: Color = MaterialTheme.colorScheme.onBackground,
    size: Int = 20,
) {
    val still = reducedMotion()
    Text(
        text.uppercase(),
        modifier,
        color = color,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        style = TextStyle(
            fontFamily = FontFamily.SansSerif,
            fontWeight = FontWeight.Black,
            fontSize = size.sp,
            letterSpacing = (-0.02 * size).sp,
            shadow = if (still) null else Shadow(color, Offset.Zero, 2f),
        ),
    )
}

// -- the Toughbook input ------------------------------------------------------

/**
 * §3's input: an inset `--void` field, 2 px `--edge` border, four 3 px "screw"
 * dots in `--kevlar` at the corners. Focus *thickens* the border to a 2 px cyan
 * ring — §7 says never remove the focus indicator, so this replaces it rather
 * than suppressing it.
 *
 * [BasicTextField] rather than `OutlinedTextField`: the Material one carries its
 * own rounded container and label animation, which is the look being replaced.
 */
@Composable
internal fun ToughbookField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = "",
    singleLine: Boolean = true,
    minHeight: Dp = 44.dp,
    enabled: Boolean = true,
) {
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val border = if (focused) Palette.Cyan else Palette.Edge

    BasicTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier,
        enabled = enabled,
        singleLine = singleLine,
        textStyle = TextStyle(
            fontFamily = FontFamily.Monospace,
            fontSize = 13.sp,
            color = MaterialTheme.colorScheme.onSurface,
        ),
        cursorBrush = SolidColor(Palette.Pink), // §3: the cursor is the kawaii accent
        interactionSource = interaction,
    ) { inner ->
        ToughbookFace(value, placeholder, border, singleLine, minHeight, inner)
    }
}

/**
 * [ToughbookField] over a [TextFieldValue].
 *
 * The composer's markdown toolbar has to insert at the cursor, which means the
 * caller owns the selection — a `String`-valued field cannot express that, and
 * rebuilding a `TextFieldValue` from the text on every keystroke would slam the
 * caret to the end and make it impossible to edit anywhere but the tail.
 */
@Composable
internal fun ToughbookField(
    value: TextFieldValue,
    onValueChange: (TextFieldValue) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = "",
    singleLine: Boolean = true,
    minHeight: Dp = 44.dp,
    enabled: Boolean = true,
) {
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val border = if (focused) Palette.Cyan else Palette.Edge

    BasicTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier,
        enabled = enabled,
        singleLine = singleLine,
        textStyle = TextStyle(
            fontFamily = FontFamily.Monospace,
            fontSize = 13.sp,
            color = MaterialTheme.colorScheme.onSurface,
        ),
        cursorBrush = SolidColor(Palette.Pink),
        interactionSource = interaction,
    ) { inner ->
        ToughbookFace(value.text, placeholder, border, singleLine, minHeight, inner)
    }
}

/** The shared frame: inset void face, screws, focus ring. */
@Composable
private fun ToughbookFace(
    text: String,
    placeholder: String,
    border: Color,
    singleLine: Boolean,
    minHeight: Dp,
    inner: @Composable () -> Unit,
) {
    val screw = 3.dp
    Box(
        Modifier
            .fillMaxWidth()
            .background(Palette.Void, CrateShape)
            .border(2.dp, border, CrateShape)
            .drawBehind {
                // Four screws. Inset by the border so they sit on the face,
                // not on the frame.
                val d = screw.toPx()
                val m = 3.dp.toPx()
                listOf(
                    Offset(m, m),
                    Offset(size.width - m - d, m),
                    Offset(m, size.height - m - d),
                    Offset(size.width - m - d, size.height - m - d),
                ).forEach { drawRect(Palette.Kevlar, it, Size(d, d)) }
            }
            .padding(horizontal = 10.dp, vertical = 10.dp)
            .then(if (singleLine) Modifier else Modifier.height(minHeight)),
    ) {
        if (text.isEmpty() && placeholder.isNotEmpty()) {
            Text(
                placeholder,
                color = Palette.Dim,
                style = TextStyle(fontFamily = FontFamily.Monospace, fontSize = 13.sp),
            )
        }
        inner()
    }
}

// -- the radio switch ---------------------------------------------------------

/**
 * §3's button: a chunky `--kevlar` face with a 3 px hard drop-shadow and an
 * `--amber` uppercase label. Pressing translates it 3 px down-right and drops the
 * shadow to zero — the throw is the whole point, so it is position *and* shadow,
 * not a ripple.
 *
 * Amber on olive is 4.48:1, which §1 permits for large text only. The label is
 * therefore 15 sp bold, clearing WCAG's 14 pt-bold threshold. Shrink it and the
 * button quietly becomes unreadable.
 *
 * The spec also asks for a clack and a particle burst on press. Sound is off
 * until the user enables it and there is no such setting yet, so neither is wired
 * here rather than being wired on-by-default — §7's "sound off by default" is not
 * something to get wrong once and ship.
 */
@Composable
internal fun CrateButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    face: Color = Palette.Kevlar,
    ink: Color = Palette.Amber,
) {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val throwDp = 3.dp
    val down = pressed && enabled

    Box(
        modifier
            .padding(end = throwDp, bottom = throwDp)
            .then(if (down) Modifier.offset(throwDp, throwDp) else Modifier)
            .drawBehind {
                if (!down) {
                    drawRect(
                        Color.Black.copy(alpha = 0.6f),
                        Offset(throwDp.toPx(), throwDp.toPx()),
                        size,
                    )
                }
            }
            .background(if (enabled) face else Palette.Asphalt, CrateShape)
            .border(2.dp, Palette.Edge, CrateShape)
            .radioClickable(interaction, enabled, onClick)
            .padding(horizontal = 14.dp, vertical = 9.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label.uppercase(),
            color = if (enabled) ink else Palette.Dim,
            style = TextStyle(
                fontFamily = FontFamily.Monospace,
                fontWeight = FontWeight.Bold,
                fontSize = 15.sp, // ≥14 pt bold — the floor for amber-on-olive
                letterSpacing = 0.05.sp,
            ),
        )
    }
}

/**
 * Clickable without Material's ripple — the throw is the feedback (§3).
 *
 * `clickable` is an extension on [Modifier], so it has to be called on the
 * receiver. Writing it fully qualified does not resolve an extension.
 */
private fun Modifier.radioClickable(
    interaction: MutableInteractionSource,
    enabled: Boolean,
    onClick: () -> Unit,
): Modifier = this.clickable(
    interactionSource = interaction,
    indication = null,
    enabled = enabled,
    onClick = onClick,
)

// -- segmented LED ------------------------------------------------------------

/**
 * §3's progress: discrete blocks, never a smooth bar, because this machine counts.
 * Used for fountain-chunk progress, where the count is the actual unit of work.
 */
@Composable
internal fun SegmentedLed(
    have: Int,
    total: Int,
    modifier: Modifier = Modifier,
    segments: Int = 8,
    height: Dp = 6.dp,
    on: Color = Palette.Phosphor,
) {
    val n = segments.coerceAtLeast(1)
    val lit = if (total <= 0) 0 else ((have.toFloat() / total) * n).toInt().coerceIn(0, n)
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
        repeat(n) { i ->
            Box(
                Modifier
                    .weight(1f)
                    .height(height)
                    .background(if (i < lit) on else Palette.Kevlar),
            )
        }
    }
}

// -- badges and status dots ---------------------------------------------------

/**
 * §3's sticker. Rotation is deliberately not offered: it was rejected during
 * design review, and a rotated badge over live text is a legibility problem
 * dressed as personality.
 *
 * [bg] defaults to void so a pink badge is legal — pink on kevlar is 2.32:1 and
 * the one pairing §1 bans, which is exactly the mistake this signature prevents
 * by not defaulting the background to the crate fill.
 */
@Composable
internal fun StickerBadge(
    text: String,
    modifier: Modifier = Modifier,
    ink: Color = Palette.Amber,
    bg: Color = Palette.Void,
    edge: Color = ink,
) {
    Text(
        text,
        modifier
            .background(bg, CrateShape)
            .border(2.dp, edge, CrateShape)
            .padding(horizontal = 6.dp, vertical = 2.dp),
        color = ink,
        maxLines = 1,
        style = TextStyle(
            fontFamily = FontFamily.Monospace,
            fontWeight = FontWeight.Bold,
            fontSize = 10.sp,
        ),
    )
}

/** Link state as a dot. Never the only signal — §1 forbids colour-alone. */
@Composable
internal fun LedDot(color: Color, modifier: Modifier = Modifier, size: Dp = 8.dp) {
    Box(modifier.size(size).background(color, CircleShape))
}

/**
 * A crate-faced toggle. Material's `Switch` is a rounded pill with a ripple; this
 * is the same control drawn as hardware.
 */
@Composable
internal fun CrateSwitch(
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    val interaction = remember { MutableInteractionSource() }
    Box(
        modifier
            .size(width = 40.dp, height = 22.dp)
            .background(if (checked) Palette.Phosphor else Palette.Kevlar, CrateShape)
            .border(2.dp, Palette.Edge, CrateShape)
            .radioClickable(interaction, enabled) { onCheckedChange(!checked) }
            .padding(2.dp),
        contentAlignment = if (checked) Alignment.CenterEnd else Alignment.CenterStart,
    ) {
        Box(Modifier.size(width = 14.dp, height = 14.dp).background(Palette.Void, CrateShape))
    }
}

/** A section rule for grouped lists (Radio / Network / Web on Bridges). */
@Composable
internal fun SectionLabel(text: String, modifier: Modifier = Modifier) {
    Text(
        text.uppercase(),
        modifier.padding(top = 10.dp, bottom = 4.dp),
        color = Palette.Dim,
        style = TextStyle(
            fontFamily = FontFamily.Monospace,
            fontSize = 10.sp,
            letterSpacing = 0.08.sp,
        ),
    )
}

/** Caption text — the `--dim` role, 4.68:1 on void. */
@Composable
internal fun Caption(text: String, modifier: Modifier = Modifier, color: Color = Palette.Dim) {
    Text(
        text,
        modifier,
        color = color,
        style = TextStyle(fontFamily = FontFamily.Monospace, fontSize = 11.sp),
    )
}

/**
 * A modal yes/no for an action that can't be taken back — a PUBLIC broadcast, say.
 * The confirm button carries the pink CTA (pink face, void ink — never pink on
 * kevlar); Cancel is the quiet default and dismissing the dialog also cancels.
 */
@Composable
internal fun ConfirmDialog(
    title: String,
    body: String,
    confirmLabel: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { DisplayHeading(title, size = 15) },
        text = { Caption(body) },
        confirmButton = { CrateButton(confirmLabel, onConfirm, face = Palette.Pink, ink = Palette.Void) },
        dismissButton = { CrateButton("Cancel", onDismiss) },
        containerColor = Palette.Asphalt,
    )
}

/** Horizontal gap, spelled once. */
@Composable
internal fun HGap(w: Dp = 8.dp) = Spacer(Modifier.width(w))

/** Vertical gap, spelled once. */
@Composable
internal fun VGap(h: Dp = 8.dp) = Spacer(Modifier.height(h))
