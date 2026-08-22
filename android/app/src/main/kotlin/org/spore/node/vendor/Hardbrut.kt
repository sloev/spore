// Vendored from https://supernihil.github.io/hardbrut/Hardbrut.kt — do not hand-edit.
// Re-pull with `python3 android/hardbrut-sync.py`; the only changes on the way
// in are this header, the `package` line below, and any compile fixes for real
// upstream bugs the script documents and applies (see its own docstring).
//
// Hardbrut.kt — HARDBRUT design system for Jetpack Compose
// Drop this file into your Android project. One dependency: Compose.
// Usage:
//   val accent = Color(0xFFFFD23F)   // yellow (or your chosen accent)
//   val accentInk = Color(0xFF000000) // black on yellow
//   HardbrutButton(onClick = { }, accent = accent, accentInk = accentInk) { Text("CLICK") }

package org.spore.node.vendor

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.*
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Text
import androidx.compose.material3.ProvideTextStyle
import androidx.compose.material.ripple.rememberRipple
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog

// =====================================================================
// TOKENS — match CSS :root variables
// =====================================================================
object HardbrutTokens {
    val Ink = Color(0xFF000000)
    val Paper = Color(0xFFFFFFFF)
    val Background = Color(0xFFFDFAF2)
    val Muted = Color(0xFF666666)
    val Space = 24.dp
    val SpaceSm = 12.dp
    val SpaceLg = 40.dp
    val Border = 3.dp
    val Shadow = 5.dp
    val ShadowSm = 3.dp
    val ShadowLg = 7.dp

    // Accent presets
    val AccentYellow  = Color(0xFFFFD23F) to Color(0xFF000000)
    val AccentRed     = Color(0xFFFF4444) to Color(0xFFFFFFFF)
    val AccentBlue    = Color(0xFF4488FF) to Color(0xFFFFFFFF)
    val AccentGreen   = Color(0xFF44CC44) to Color(0xFF000000)
    val AccentPink    = Color(0xFFFF66AA) to Color(0xFF000000)
    val AccentPurple  = Color(0xFFAA66FF) to Color(0xFFFFFFFF)
}

// =====================================================================
// HARD SHADOW — zero blur, like CSS box-shadow: X X 0 #000
// =====================================================================
fun Modifier.hardShadow(size: Dp, color: Color = HardbrutTokens.Ink): Modifier = this.then(
    drawBehind {
        drawRect(
            color = color,
            topLeft = Offset(size.toPx(), size.toPx()),
            size = Size(this.size.width, this.size.height)
        )
    }
)

// =====================================================================
// BUTTON — two kinds: Default (accent fill) + Cancel (white fill)
// =====================================================================
enum class ButtonKind { Default, Cancel }

@Composable
@Suppress("DEPRECATION_ERROR") // rememberRipple: still the only ripple API that resolves against compose-bom 2024.09.02
fun HardbrutButton(
    onClick: () -> Unit,
    accent: Color,
    accentInk: Color,
    kind: ButtonKind = ButtonKind.Default,
    enabled: Boolean = true,
    modifier: Modifier = Modifier,
    content: @Composable RowScope.() -> Unit
) {
    val shadow = when (kind) {
        ButtonKind.Default -> HardbrutTokens.Shadow
        ButtonKind.Cancel -> HardbrutTokens.ShadowSm
    }
    val bg = when (kind) {
        ButtonKind.Default -> accent
        ButtonKind.Cancel -> HardbrutTokens.Paper
    }
    val fg = when (kind) {
        ButtonKind.Default -> accentInk
        ButtonKind.Cancel -> HardbrutTokens.Ink
    }
    val alpha = if (enabled) 1f else 0.45f
    val interactionSource = remember { MutableInteractionSource() }

    Row(
        modifier = modifier
            .hardShadow(shadow)
            .border(HardbrutTokens.Border, HardbrutTokens.Ink)
            .background(bg)
            .clickable(
                enabled = enabled,
                interactionSource = interactionSource,
                indication = rememberRipple(color = HardbrutTokens.Ink.copy(alpha = 0.15f))
            ) { onClick() }
            .padding(horizontal = 24.dp, vertical = 12.dp),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically
    ) {
        // Compose has no CSS-like text-transform — callers pass already-uppercase
        // labels (Text("SUBMIT")), same as every example in this file.
        ProvideTextStyle(
            TextStyle(
                fontWeight = FontWeight.ExtraBold,
                fontSize = 15.sp,
                letterSpacing = 0.3.sp,
                color = fg.copy(alpha = alpha)
            )
        ) { content() }
    }
}

// =====================================================================
// CARD
// =====================================================================
@Composable
fun HardbrutCard(
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit
) {
    Column(
        modifier = modifier
            .hardShadow(HardbrutTokens.Shadow)
            .border(HardbrutTokens.Border, HardbrutTokens.Ink)
            .background(HardbrutTokens.Paper)
            .padding(HardbrutTokens.Space)
    ) { content() }
}

// =====================================================================
// LIST ROW — e.g. a conversation list. Mirrors CSS .list-row.
// =====================================================================
@Composable
@Suppress("DEPRECATION_ERROR") // rememberRipple: still the only ripple API that resolves against compose-bom 2024.09.02
fun HardbrutListRow(
    title: String,
    subtitle: String? = null,
    avatarText: String? = null,
    onClick: (() -> Unit)? = null,
    trailing: @Composable (RowScope.() -> Unit)? = null,
    modifier: Modifier = Modifier
) {
    val interactionSource = remember { MutableInteractionSource() }
    val clickModifier = if (onClick != null) {
        Modifier.clickable(
            interactionSource = interactionSource,
            indication = rememberRipple(color = HardbrutTokens.Ink.copy(alpha = 0.15f))
        ) { onClick() }
    } else Modifier

    Row(
        modifier = modifier
            .fillMaxWidth()
            .hardShadow(HardbrutTokens.ShadowSm)
            .border(HardbrutTokens.Border, HardbrutTokens.Ink)
            .background(HardbrutTokens.Paper)
            .then(clickModifier)
            .padding(horizontal = HardbrutTokens.Space, vertical = HardbrutTokens.SpaceSm),
        horizontalArrangement = Arrangement.spacedBy(HardbrutTokens.SpaceSm),
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (avatarText != null) {
            Box(
                modifier = Modifier
                    .size(44.dp)
                    .border(2.dp, HardbrutTokens.Ink)
                    .background(HardbrutTokens.Background),
                contentAlignment = Alignment.Center
            ) {
                Text(avatarText.uppercase(), fontWeight = FontWeight.ExtraBold, fontSize = 13.sp)
            }
        }
        Column(modifier = Modifier.weight(1f)) {
            Text(title, fontWeight = FontWeight.ExtraBold, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (subtitle != null) {
                Text(subtitle, color = HardbrutTokens.Muted, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        trailing?.invoke(this)
    }
}

// =====================================================================
// TEXT FIELD
// =====================================================================
@Composable
fun HardbrutTextField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    label: String = "",
    placeholder: String = ""
) {
    var focused by remember { mutableStateOf(false) }
    val shadow = if (focused) HardbrutTokens.Shadow else HardbrutTokens.ShadowSm

    Column(modifier = modifier) {
        if (label.isNotEmpty()) {
            Text(
                text = label.uppercase(),
                fontWeight = FontWeight.ExtraBold,
                fontSize = 13.sp,
                letterSpacing = 0.4.sp,
                color = HardbrutTokens.Ink,
                modifier = Modifier.padding(bottom = 6.dp)
            )
        }
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            modifier = Modifier
                .hardShadow(shadow)
                .border(HardbrutTokens.Border, HardbrutTokens.Ink)
                .background(HardbrutTokens.Paper)
                .onFocusChanged { focused = it.isFocused }
                .padding(horizontal = 14.dp, vertical = 11.dp)
                .heightIn(min = 46.dp),
            textStyle = TextStyle(
                fontSize = 15.sp,
                color = HardbrutTokens.Ink
            ),
            decorationBox = { inner ->
                Box {
                    if (value.isEmpty()) {
                        Text(
                            text = placeholder,
                            color = HardbrutTokens.Muted,
                            fontSize = 15.sp
                        )
                    }
                    inner()
                }
            }
        )
    }
}

// =====================================================================
// CHECKBOX
// =====================================================================
@Composable
fun HardbrutCheckbox(
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit
) {
    Box(
        modifier = Modifier
            .size(22.dp)
            .hardShadow(HardbrutTokens.ShadowSm)
            .border(HardbrutTokens.Border, HardbrutTokens.Ink)
            .background(HardbrutTokens.Paper)
            .clickable { onCheckedChange(!checked) },
        contentAlignment = Alignment.Center
    ) {
        if (checked) {
            Text("✓", fontWeight = FontWeight.ExtraBold, fontSize = 14.sp)
        }
    }
}

// =====================================================================
// RADIO BUTTON
// =====================================================================
@Composable
fun HardbrutRadio(
    selected: Boolean,
    onClick: () -> Unit
) {
    Box(
        modifier = Modifier
            .size(22.dp)
            .hardShadow(HardbrutTokens.ShadowSm)
            .border(HardbrutTokens.Border, HardbrutTokens.Ink)
            .background(HardbrutTokens.Paper, CircleShape)
            .clickable { onClick() },
        contentAlignment = Alignment.Center
    ) {
        if (selected) {
            Box(
                modifier = Modifier
                    .size(8.dp)
                    .background(HardbrutTokens.Ink, CircleShape)
            )
        }
    }
}

// =====================================================================
// SWITCH — hard-edged sliding block, not an iOS-style rounded pill.
// =====================================================================
@Composable
fun HardbrutSwitch(
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    accent: Color = HardbrutTokens.AccentYellow.first,
    accentInk: Color = HardbrutTokens.AccentYellow.second
) {
    val trackColor = if (checked) accent else HardbrutTokens.Paper
    val thumbColor = if (checked) accentInk else HardbrutTokens.Ink
    val thumbOffset by animateDpAsState(if (checked) 22.dp else 2.dp, label = "HardbrutSwitchThumb")

    Box(
        modifier = Modifier
            .size(width = 48.dp, height = 26.dp)
            .hardShadow(HardbrutTokens.ShadowSm)
            .border(HardbrutTokens.Border, HardbrutTokens.Ink)
            .background(trackColor)
            .clickable { onCheckedChange(!checked) }
    ) {
        Box(
            modifier = Modifier
                .offset(x = thumbOffset, y = 2.dp)
                .size(16.dp)
                .background(thumbColor)
        )
    }
}

// =====================================================================
// BADGE
// =====================================================================
@Composable
fun HardbrutBadge(
    text: String,
    accent: Color = HardbrutTokens.AccentYellow.first,
    accentInk: Color = HardbrutTokens.AccentYellow.second
) {
    Text(
        text = text.uppercase(),
        fontFamily = FontFamily.Monospace,
        fontSize = 12.sp,
        letterSpacing = 0.6.sp,
        color = accentInk,
        modifier = Modifier
            .hardShadow(HardbrutTokens.ShadowSm)
            .border(2.dp, HardbrutTokens.Ink)
            .background(accent)
            .padding(horizontal = 10.dp, vertical = 3.dp)
    )
}

// =====================================================================
// CHIP — selectable/toggleable pill (filter preset, topic tag).
// =====================================================================
@Composable
@Suppress("DEPRECATION_ERROR") // rememberRipple: still the only ripple API that resolves against compose-bom 2024.09.02
fun HardbrutChip(
    text: String,
    selected: Boolean,
    onClick: () -> Unit,
    accent: Color = HardbrutTokens.AccentYellow.first,
    accentInk: Color = HardbrutTokens.AccentYellow.second
) {
    val bg = if (selected) accent else HardbrutTokens.Paper
    val fg = if (selected) accentInk else HardbrutTokens.Ink
    val interactionSource = remember { MutableInteractionSource() }

    Text(
        text = text.uppercase(),
        fontFamily = FontFamily.Monospace,
        fontSize = 12.sp,
        letterSpacing = 0.5.sp,
        color = fg,
        modifier = Modifier
            .hardShadow(HardbrutTokens.ShadowSm)
            .border(2.dp, HardbrutTokens.Ink)
            .background(bg)
            .clickable(
                interactionSource = interactionSource,
                indication = rememberRipple(color = HardbrutTokens.Ink.copy(alpha = 0.15f))
            ) { onClick() }
            .padding(horizontal = 12.dp, vertical = 5.dp)
    )
}

// =====================================================================
// TAB ROW — panel switching is left to the caller (a when/if on the
// selected index), same as CSS leaves [hidden] toggling to the caller.
// =====================================================================
@Composable
@Suppress("DEPRECATION_ERROR") // rememberRipple: still the only ripple API that resolves against compose-bom 2024.09.02
fun HardbrutTabRow(
    tabs: List<String>,
    selected: Int,
    onSelect: (Int) -> Unit,
    accent: Color = HardbrutTokens.AccentYellow.first,
    accentInk: Color = HardbrutTokens.AccentYellow.second
) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        tabs.forEachIndexed { index, label ->
            val isSelected = index == selected
            val bg = if (isSelected) accent else HardbrutTokens.Paper
            val fg = if (isSelected) accentInk else HardbrutTokens.Ink
            val interactionSource = remember { MutableInteractionSource() }

            Text(
                text = label.uppercase(),
                fontWeight = FontWeight.ExtraBold,
                fontSize = 13.sp,
                letterSpacing = 0.3.sp,
                color = fg,
                modifier = Modifier
                    .hardShadow(HardbrutTokens.ShadowSm)
                    .border(2.dp, HardbrutTokens.Ink)
                    .background(bg)
                    .clickable(
                        interactionSource = interactionSource,
                        indication = rememberRipple(color = HardbrutTokens.Ink.copy(alpha = 0.15f))
                    ) { onSelect(index) }
                    .padding(horizontal = 20.dp, vertical = 10.dp)
            )
        }
    }
}

// =====================================================================
// DIALOG — a custom-styled Dialog window, not AlertDialog: Material3's
// AlertDialog imposes its own shape/elevation defaults that fight the
// zero-radius/hard-shadow look, same reason every other composable here
// is built from primitives instead of wrapping a Material3 component.
// =====================================================================
@Composable
fun HardbrutDialog(
    onDismissRequest: () -> Unit,
    content: @Composable ColumnScope.() -> Unit
) {
    Dialog(onDismissRequest = onDismissRequest) {
        Column(
            modifier = Modifier
                .hardShadow(HardbrutTokens.ShadowLg)
                .border(HardbrutTokens.Border, HardbrutTokens.Ink)
                .background(HardbrutTokens.Paper)
                .padding(HardbrutTokens.Space)
        ) { content() }
    }
}

// =====================================================================
// CALLOUT
// =====================================================================
@Composable
fun HardbrutCallout(
    text: String,
    accent: Color = HardbrutTokens.AccentYellow.first,
    accentInk: Color = HardbrutTokens.AccentYellow.second,
    modifier: Modifier = Modifier
) {
    Text(
        text = text,
        fontWeight = FontWeight.Bold,
        color = accentInk,
        modifier = modifier
            .hardShadow(HardbrutTokens.ShadowSm)
            .border(HardbrutTokens.Border, HardbrutTokens.Ink)
            .background(accent)
            .padding(HardbrutTokens.Space)
    )
}

// =====================================================================
// DETAILS (expandable)
// =====================================================================
@Composable
fun HardbrutDetails(
    summary: String,
    expanded: Boolean,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit
) {
    val shadow = if (expanded) HardbrutTokens.Shadow else HardbrutTokens.ShadowSm
    Column(
        modifier = modifier
            .hardShadow(shadow)
            .border(HardbrutTokens.Border, HardbrutTokens.Ink)
            .background(HardbrutTokens.Paper)
    ) {
        Row(
            modifier = Modifier
                .clickable { onToggle() }
                .padding(horizontal = HardbrutTokens.Space, vertical = HardbrutTokens.SpaceSm),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                text = if (expanded) "−" else "+",
                fontFamily = FontFamily.Monospace,
                fontSize = 16.sp
            )
            Spacer(Modifier.width(8.dp))
            Text(
                text = summary.uppercase(),
                fontWeight = FontWeight.ExtraBold,
                fontSize = 15.sp,
                letterSpacing = 0.3.sp
            )
        }
        if (expanded) {
            Column(modifier = Modifier.padding(HardbrutTokens.Space)) {
                content()
            }
        }
    }
}
