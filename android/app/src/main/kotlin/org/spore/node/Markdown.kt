package org.spore.node

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withStyle

/**
 * The smallest markdown that is still worth having: `**bold**`, `*italic*` or
 * `_italic_`, `` `code` ``, and `[text](url)`.
 *
 * **Inline only, on purpose.** No headings, lists, block quotes or fences. A feed
 * post is a sentence or two on a mesh with a per-envelope size budget, and a full
 * parser here would be a dependency and a fuzz target for no gain. If a post ever
 * needs a bulleted list, that is the moment to reconsider — not before.
 *
 * Unmatched delimiters are left as literal text rather than swallowed. Someone
 * writing `2 * 3 * 4` gets what they typed, and a post that arrives truncated
 * mid-token still reads.
 */
object Markdown {

    private val CODE_INK = Palette.Cyan
    private val LINK_INK = Palette.Phosphor

    /** Render inline markdown for display. */
    fun render(src: String, base: Color): AnnotatedString = buildAnnotatedString {
        var i = 0
        val n = src.length
        while (i < n) {
            when {
                src.startsWith("**", i) -> {
                    val end = src.indexOf("**", i + 2)
                    if (end < 0) { append(src[i]); i++ }
                    else {
                        withStyle(SpanStyle(fontWeight = FontWeight.Bold)) {
                            append(src.substring(i + 2, end))
                        }
                        i = end + 2
                    }
                }
                src[i] == '`' -> {
                    val end = src.indexOf('`', i + 1)
                    if (end < 0) { append(src[i]); i++ }
                    else {
                        withStyle(SpanStyle(fontFamily = FontFamily.Monospace, color = CODE_INK)) {
                            append(src.substring(i + 1, end))
                        }
                        i = end + 1
                    }
                }
                src[i] == '*' || src[i] == '_' -> {
                    val mark = src[i]
                    val end = src.indexOf(mark, i + 1)
                    // A lone delimiter, or an empty pair, is literal text.
                    if (end < 0 || end == i + 1) { append(src[i]); i++ }
                    else {
                        withStyle(SpanStyle(fontStyle = FontStyle.Italic)) {
                            append(src.substring(i + 1, end))
                        }
                        i = end + 1
                    }
                }
                src[i] == '[' -> {
                    val close = src.indexOf(']', i + 1)
                    val open = if (close >= 0 && close + 1 < n && src[close + 1] == '(') close + 1 else -1
                    val end = if (open >= 0) src.indexOf(')', open + 1) else -1
                    if (end < 0) { append(src[i]); i++ }
                    else {
                        val label = src.substring(i + 1, close)
                        val url = src.substring(open + 1, end)
                        // The URL rides along as an annotation so a caller can make
                        // it tappable; nothing here opens it, because a link in a
                        // signed-but-public post is attacker-controlled text.
                        pushStringAnnotation("url", url)
                        withStyle(SpanStyle(color = LINK_INK, textDecoration = TextDecoration.Underline)) {
                            append(label.ifEmpty { url })
                        }
                        pop()
                        i = end + 1
                    }
                }
                else -> { append(src[i]); i++ }
            }
        }
        addStyle(SpanStyle(color = base), 0, length)
    }

    /**
     * Wrap the selection in [mark], or insert an empty pair at the cursor and put
     * the caret between the halves — what every editor's bold button does.
     */
    fun wrap(v: TextFieldValue, mark: String): TextFieldValue {
        val s = v.selection
        val text = v.text
        val a = minOf(s.start, s.end).coerceIn(0, text.length)
        val b = maxOf(s.start, s.end).coerceIn(0, text.length)
        val out = text.substring(0, a) + mark + text.substring(a, b) + mark + text.substring(b)
        val caret = if (a == b) a + mark.length else b + 2 * mark.length
        return TextFieldValue(out, androidx.compose.ui.text.TextRange(caret))
    }

    /** Insert a link skeleton, using the selection as the label when there is one. */
    fun link(v: TextFieldValue): TextFieldValue {
        val s = v.selection
        val text = v.text
        val a = minOf(s.start, s.end).coerceIn(0, text.length)
        val b = maxOf(s.start, s.end).coerceIn(0, text.length)
        val label = text.substring(a, b).ifEmpty { "text" }
        val ins = "[$label](url)"
        val out = text.substring(0, a) + ins + text.substring(b)
        // Caret lands on "url" so the next keystroke replaces the placeholder.
        val urlAt = a + ins.length - 4
        return TextFieldValue(out, androidx.compose.ui.text.TextRange(urlAt, urlAt + 3))
    }

    // -- image attachments --------------------------------------------------

    /**
     * How an attached image rides along with a post: an ordinary markdown image
     * whose URL is the SPORE magnet the bytes were published under.
     *
     * A post is one signed envelope carrying UTF-8, so the image cannot be *in*
     * it — the bytes go through the same manifest-and-chunk path as any file, and
     * this line is the pointer. A reader without the chunks yet sees the transfer
     * progress; a reader on a client that does not understand the marker sees a
     * plain markdown image link, which is a reasonable thing to see.
     */
    private val IMAGE = Regex("""!\[[^\]]*]\(spore:([0-9a-fA-F]{16,})\)""")

    fun imageMarker(name: String, magnet: String): String = "\n\n![$name](spore:$magnet)"

    /** The magnet an attached image was published under, if the post has one. */
    fun imageMagnet(body: String): String? = IMAGE.find(body)?.groupValues?.get(1)

    /** The post body with the image marker taken out, for display. */
    fun stripImage(body: String): String = IMAGE.replace(body, "").trim()

    // -- chat attachments ---------------------------------------------------

    /**
     * A chat attachment, parsed from a message body.
     *
     * Chat carries the mime type as well as the magnet, because a chat bubble
     * decides whether to render an inline image or a file chip and the feed's
     * `![](spore:…)` form has nowhere to put it. This is a strictly richer marker,
     * kept distinct from the feed's image form on purpose (§ UX-ISSUES).
     */
    data class Attach(val name: String, val magnet: String, val mime: String)

    // Canonical marker, always the last line of the body:
    //   📎 <filename> | spore:<hex-magnet> | <mime>
    // Application convention only — relays see opaque UTF-8. `(?m)` so `$` matches
    // the end of that line rather than only the end of the whole body.
    private val ATTACH = Regex("""(?m)^📎 (.+) \| spore:([0-9a-fA-F]{16,}) \| (\S+)$""")

    /** Build the marker line for a published attachment. */
    fun attachMarker(name: String, magnet: String, mime: String): String =
        "📎 $name | spore:$magnet | $mime"

    /**
     * Split a body into its human text and its attachment, if it carries one.
     * The text has the marker line removed and is trimmed; a body that is only a
     * marker yields empty text. A body with no marker yields `(body, null)`.
     */
    fun parseAttach(body: String): Pair<String, Attach?> {
        val m = ATTACH.find(body) ?: return body to null
        val att = Attach(m.groupValues[1], m.groupValues[2], m.groupValues[3])
        val text = body.replace(m.value, "").trim()
        return text to att
    }
}
