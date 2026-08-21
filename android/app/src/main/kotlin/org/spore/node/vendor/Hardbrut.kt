// Vendored from https://supernihil.github.io/hardbrut/Hardbrut.kt — do not hand-edit.
// Re-pull with `python3 android/hardbrut-sync.py`; the only changes on the way
// in are this header, the `package` line below, and the compile fixes for real
// upstream bugs the script documents and applies (see its own docstring).
//
// Hardbrut.kt — HARDBRUT design system for Jetpack Compose
// Drop this file into your Android project. One dependency: Compose.
// Usage:
//   val accent = Color(0xFFFFD23F)   // yellow (or your chosen accent)
//   val accentInk = Color(0xFF000000) // black on yellow
//   HardbrutButton(onClick = { }, accent = accent, accentInk = accentInk) { Text("CLICK") }

package org.spore.node.vendor

import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.ProvideTextStyle
import androidx.compose.material3.Text
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

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

    Row(
        modifier = modifier
            .hardShadow(shadow)
            .border(HardbrutTokens.Border, HardbrutTokens.Ink)
            .background(bg)
            .clickable(enabled = enabled) { onClick() }
            .padding(horizontal = 24.dp, vertical = 12.dp),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically
    ) {
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
