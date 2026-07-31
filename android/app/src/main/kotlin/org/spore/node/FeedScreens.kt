package org.spore.node

/*
 * Feed: the topic timeline, a post, and the composer.
 *
 * Post bodies render inline markdown (Markdown.kt) and may reference an image by
 * the magnet its bytes were published under. See Markdown.imageMarker for why an
 * image cannot simply be inside the post.
 */

import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
internal fun FeedScreen(compose: (String) -> Unit) {
    val posts by NodeController.posts.collectAsState()
    val topics by NodeController.topics.collectAsState()
    var follow by remember { mutableStateOf("") }
    var activeTopic by remember { mutableStateOf<String?>(null) }
    val shown = remember(posts, activeTopic) {
        if (activeTopic == null) posts else posts.filter { it.topic == activeTopic }
    }

    Column(Modifier.padding(horizontal = 12.dp).fillMaxSize()) {
        Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            ToughbookField(follow, { follow = it }, Modifier.weight(1f), placeholder = "follow a topic, e.g. spore/news")
            HGap()
            CrateButton("Follow", { NodeController.follow(follow); follow = "" }, enabled = follow.isNotBlank())
        }
        // Topic chips read across, not down — a one-line filter strip.
        LazyRow(Modifier.fillMaxWidth().padding(vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            items(listOf<String?>(null) + topics) { t ->
                TopicChip(
                    if (t == null) "all" else "#$t",
                    selected = activeTopic == t,
                    onClick = { activeTopic = t },
                )
            }
        }
        if (shown.isEmpty()) {
            Caption("nothing sprouting here yet 🌱", Modifier.fillMaxWidth().padding(24.dp))
        }
        LazyColumn(Modifier.weight(1f)) {
            items(shown.asReversed()) { p -> PostCard(p) }
        }
        val target = activeTopic ?: topics.firstOrNull()
        Row(Modifier.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Caption(
                if (target != null) "posting to #$target" else "follow a topic first",
                Modifier.weight(1f),
            )
            CrateButton(
                "New post",
                { target?.let(compose) },
                enabled = target != null,
                face = Palette.Pink,
                ink = Palette.Void,
            )
        }
    }
}

@Composable
private fun TopicChip(label: String, selected: Boolean, onClick: () -> Unit) {
    // Selected is pink-on-void with a pink edge; unselected is amber on kevlar,
    // which §1 allows for chrome at 4.48:1. Pink on kevlar — the combination the
    // mock reached for — is 2.32:1 and never used.
    StickerBadge(
        label,
        // .selectable (not .clickable) announces selected/unselected to TalkBack —
        // colour alone (pink vs amber) doesn't (B7).
        Modifier.selectable(selected = selected, onClick = onClick, role = Role.Tab).padding(vertical = 2.dp),
        ink = if (selected) Palette.Pink else Palette.Amber,
        bg = if (selected) Palette.Void else Palette.Kevlar,
        edge = if (selected) Palette.Pink else Palette.Edge,
    )
}

@Composable
private fun PostCard(p: Post) {
    val paths by NodeController.filePaths.collectAsState()
    val transfers by NodeController.transfers.collectAsState()
    val magnet = remember(p.text) { Markdown.imageMagnet(p.text) }
    val body = remember(p.text) { Markdown.stripImage(p.text) }

    Crate(Modifier.fillMaxWidth()) {
        Column {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(Petnames.label(p.author), Modifier.weight(1f), color = Palette.Amber, fontWeight = FontWeight.Bold)
                if (p.verified) StickerBadge("🔒 verified", ink = Palette.Phosphor)
                else StickerBadge("⚠ unverified", ink = Palette.Pink)
            }
            Caption("#${p.topic} · ${timeOf(p.ts)}")
            VGap(6.dp)
            Text(Markdown.render(body, Palette.Amber))
            if (magnet != null) {
                VGap(8.dp)
                InlineImage(magnet, paths[magnet], transfers.firstOrNull { it.magnet == magnet })
            }
        }
    }
}

/**
 * An attached image, or its arrival.
 *
 * Decoded with `inSampleSize` rather than at full resolution: a phone photo is
 * tens of megapixels and a feed row is a couple of hundred dp, so decoding it
 * whole would spend ~100 MB of heap to draw a thumbnail. No image library —
 * adding Coil for one call site is a dependency this app does not need.
 */
@Composable
private fun InlineImage(magnet: String, path: String?, transfer: Transfer?) {
    if (path == null) {
        Column(Modifier.fillMaxWidth()) {
            Caption(if (transfer == null) "📎 image not here yet" else "📎 fetching image")
            VGap(4.dp)
            SegmentedLed(transfer?.have ?: 0, transfer?.count ?: 1, Modifier.fillMaxWidth())
        }
        return
    }
    // Decoded on IO, not in composition: a JPEG decode is tens of milliseconds and
    // this runs inside a scrolling list. `produceState` keeps the row rendering
    // while it happens instead of stalling the frame.
    val bmp by androidx.compose.runtime.produceState<android.graphics.Bitmap?>(null, path) {
        value = withContext(Dispatchers.IO) {
            runCatching {
                val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
                android.graphics.BitmapFactory.decodeFile(path, bounds)
                var scale = 1
                // Halve until the long edge is under 1080 px. A phone photo is tens
                // of megapixels; decoding it whole to fill a 220 dp row would spend
                // ~100 MB of heap on a thumbnail.
                while (bounds.outWidth / scale > 1080) scale *= 2
                android.graphics.BitmapFactory.decodeFile(
                    path,
                    android.graphics.BitmapFactory.Options().apply { inSampleSize = scale },
                )
            }.getOrNull()
        }
    }
    val shown = bmp
    if (shown == null) {
        Caption("📎 decoding image…")
        return
    }
    Image(
        shown.asImageBitmap(),
        contentDescription = "attached image",
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(max = 220.dp)
            .border(2.dp, Palette.Edge),
        contentScale = ContentScale.Crop,
    )
}

/**
 * The mock's Compose Post screen: topic chips, a markdown toolbar, a body field,
 * a character counter and an image slot.
 *
 * The toolbar buttons insert real syntax at the cursor and [PostCard] renders the
 * result, because a toolbar whose buttons do nothing is worse than no toolbar —
 * it tells the user a feature exists and then does not provide it.
 */
@Composable
internal fun ComposePost(topic: String, done: () -> Unit) {
    val ctx = LocalContext.current
    val topics by NodeController.topics.collectAsState()
    val confirm = rememberConfirm()
    var target by remember { mutableStateOf(topic) }
    var body by remember { mutableStateOf(TextFieldValue("")) }
    var image by remember { mutableStateOf<Pair<String, ByteArray>?>(null) }

    val pickImage = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri: Uri? ->
        if (uri != null) {
            val name = ctx.contentResolver.query(uri, null, null, null, null)?.use { c ->
                val i = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (c.moveToFirst() && i >= 0) c.getString(i) else null
            } ?: "image.jpg"
            val data = ctx.contentResolver.openInputStream(uri)?.use { it.readBytes() }
            image = if (data != null) name to data else null
        }
    }

    val limit = 500

    Column(
        Modifier.padding(12.dp).fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        SectionLabel("Topic")
        LazyRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            items(topics) { t ->
                TopicChip("#$t", selected = t == target, onClick = { target = t })
            }
        }

        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            CrateButton("B", { body = Markdown.wrap(body, "**") }, contentDescription = "Bold")
            CrateButton("i", { body = Markdown.wrap(body, "_") }, contentDescription = "Italic")
            CrateButton("</>", { body = Markdown.wrap(body, "`") }, contentDescription = "Code")
            CrateButton("🔗", { body = Markdown.link(body) }, contentDescription = "Insert link")
            CrateButton("🖼", { pickImage.launch("image/*") }, contentDescription = "Add image")
        }

        ToughbookField(
            body,
            // Cap by refusing the edit rather than truncating it: truncating moves
            // the caret and eats the character you typed in the middle of a
            // sentence, which reads as the field being broken.
            { v -> if (v.text.length <= limit) body = v },
            Modifier.fillMaxWidth(),
            placeholder = "what happened?",
            singleLine = false,
            minHeight = 120.dp,
        )

        image?.let { (name, bytes) ->
            Crate(Modifier.fillMaxWidth()) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(name, color = Palette.Amber, maxLines = 1)
                        Caption("${bytes.size / 1024} KB attached")
                    }
                    CrateButton("Remove", { image = null })
                }
            }
        }

        Row(verticalAlignment = Alignment.CenterVertically) {
            Caption("${body.text.length} / $limit · signed with your key", Modifier.weight(1f))
            CrateButton(
                "Post",
                {
                    val img = image
                    val ok = if (img == null) {
                        NodeController.post(target, body.text)
                    } else {
                        NodeController.postWithImage(target, body.text, img.first, img.second)
                    }
                    if (ok) { confirm("Posted to #$target"); done() }
                    else if (img == null) confirm("Node not started yet — not posted")
                    else confirm("Image too large for this node's store — post it smaller")
                },
                enabled = body.text.isNotBlank() || image != null,
                face = Palette.Pink,
                ink = Palette.Void,
            )
        }
    }
}
