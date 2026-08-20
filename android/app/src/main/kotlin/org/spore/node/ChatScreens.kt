package org.spore.node

/*
 * Chats: the thread list, and one conversation.
 *
 * Message bubbles are crates, right-aligned when mine. Sent and received differ by
 * border colour *and* side — docs/VISUALDESIGN.md section 1 forbids signalling by
 * colour alone, and the alignment is what carries it otherwise.
 */

import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import androidx.compose.ui.unit.dp

@Composable
internal fun ChatsList(addr: String, open: (String) -> Unit) {
    val ctx = LocalContext.current
    val messages by NodeController.messages.collectAsState()
    val names by Petnames.map.collectAsState()
    val nearby by NodeController.peers.collectAsState()
    val avatars by NodeController.peerAvatarPath.collectAsState()
    var newPeer by remember { mutableStateOf("") }

    val threads = remember(messages, names) {
        (messages.map { it.peer } + Petnames.PUBLIC + names.keys).distinct()
    }
    Column(Modifier.padding(horizontal = 12.dp).fillMaxSize()) {
        VGap()
        // Your address is what others need to reach you — make it one tap to hand over.
        Crate(Modifier.fillMaxWidth()) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Caption("your address")
                    Text(addr, color = Palette.Ink)
                }
                HGap()
                CrateButton("Copy", {
                    val cm = ctx.getSystemService(android.content.ClipboardManager::class.java)
                    cm?.setPrimaryClip(android.content.ClipData.newPlainText("SPORE address", addr))
                })
                CrateButton("Share", {
                    val i = Intent(Intent.ACTION_SEND).apply {
                        type = "text/plain"
                        putExtra(Intent.EXTRA_TEXT, "My SPORE address: $addr")
                    }
                    ctx.startActivity(Intent.createChooser(i, "Share your SPORE address"))
                })
            }
        }

        LazyColumn(Modifier.weight(1f)) {
            if (nearby.isNotEmpty()) {
                item { SectionLabel("Nearby (${nearby.size})") }
                items(nearby) { p ->
                    val ago = if (p.secondsAgo < 60) "${p.secondsAgo}s ago" else "${p.secondsAgo / 60}m ago"
                    // Your petname wins; otherwise show what they call themselves,
                    // quoted so it reads as a claim rather than a verified name.
                    val shown = names[p.addr] ?: p.announced.takeIf { it.isNotBlank() }?.let { "“$it”" }
                        ?: Petnames.label(p.addr)
                    Crate(Modifier.fillMaxWidth().clickable { open(p.addr) }) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            ProfilePic(avatars[p.addr], shown)
                            HGap(10.dp)
                            Column(Modifier.weight(1f)) {
                                Text(shown, color = Palette.Ink, fontWeight = FontWeight.Bold)
                                Caption("📡 $ago")
                            }
                            if (p.hasKey) StickerBadge("🔒 sealed", ink = Palette.Ink)
                        }
                    }
                }
            } else {
                item {
                    Caption(
                        "no spores nearby yet 📡\nadd a bridge, and anyone in range appears here",
                        Modifier.fillMaxWidth().padding(24.dp),
                    )
                }
            }

            item { SectionLabel("Conversations") }
            // `threads` always contains PUBLIC (added above), so it is never
            // literally empty — the honest "nothing here" case is no *real*
            // conversation yet, i.e. nothing besides the PUBLIC row.
            if (threads.size <= 1) {
                item {
                    Caption(
                        "no conversations yet 📡\nopen one by address below, or wait for someone nearby",
                        Modifier.fillMaxWidth().padding(vertical = 12.dp),
                    )
                }
            }
            items(threads) { peer ->
                val last = messages.lastOrNull { it.peer == peer }
                // The mock shows an unread count here. There is no read tracking in
                // the app, so a badge would be a number with nothing behind it —
                // left out rather than faked. Time and last line carry the row.
                Crate(Modifier.fillMaxWidth().clickable { open(peer) }) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        ProfilePic(avatars[peer], Petnames.label(peer))
                        HGap(10.dp)
                        Column(Modifier.weight(1f)) {
                            Row {
                                Text(
                                    Petnames.label(peer),
                                    Modifier.weight(1f),
                                    color = Palette.Ink,
                                    fontWeight = FontWeight.Bold,
                                    maxLines = 1,
                                )
                                last?.let { Caption(timeOf(it.ts)) }
                            }
                            Caption(
                                (if (last?.encrypted == true) "🔒 " else "") + (last?.text ?: "—"),
                                Modifier.fillMaxWidth(),
                            )
                        }
                    }
                }
            }
            item {
                Row(Modifier.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    ToughbookField(
                        newPeer, { newPeer = it },
                        Modifier.weight(1f), placeholder = "open chat by address (16 hex)",
                    )
                    HGap()
                    CrateButton("Open", {
                        val h = newPeer.trim().lowercase()
                        if (h.length == 16) { open(h); newPeer = "" }
                    }, enabled = newPeer.trim().length == 16)
                }
            }
        }
    }
}

@Composable
internal fun ChatDetail(peer: String) {
    val ctx = LocalContext.current
    val messages by NodeController.messages.collectAsState()
    val names by Petnames.map.collectAsState()
    var text by remember { mutableStateOf("") }
    var editingName by remember(peer) { mutableStateOf(names[peer] ?: "") }
    val thread = remember(messages, peer) { messages.filter { it.peer == peer } }
    val confirm = rememberConfirm()

    // The petname field sits above the thread and would otherwise be the first
    // stop for a keyboard/TalkBack user entering a chat, ahead of the actual
    // reason they're here. Route initial focus to the composer instead (B7).
    val composerFocus = remember { FocusRequester() }
    LaunchedEffect(peer) { runCatching { composerFocus.requestFocus() } }

    // Staged, not sent: picking a file only fills this. Nothing goes on the wire
    // or into the thread until Send — the file reads as attached to the message
    // being composed, which is the whole point of the change.
    var staged by remember(peer) { mutableStateOf<StagedAttachment?>(null) }

    val pickFile = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri: Uri? ->
        if (uri != null) {
            val name = ctx.contentResolver.query(uri, null, null, null, null)?.use { c ->
                val i = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (c.moveToFirst() && i >= 0) c.getString(i) else null
            } ?: "file.bin"
            val mime = ctx.contentResolver.getType(uri) ?: "application/octet-stream"
            val data = ctx.contentResolver.openInputStream(uri)?.use { it.readBytes() }
            if (data != null) staged = StagedAttachment(name, data, mime)
        }
    }

    // A PUBLIC send reaches every node in range rather than just this thread, and —
    // unlike a DM — is signed but never sealed, so it's readable by anyone carrying
    // it. That and "can't be unsent" earn it a confirm dialog rather than a single
    // tap. The actual send (attachment or plain text, with its own failure feedback
    // from B2) lives here so both the direct tap and the confirmed broadcast reuse it.
    var confirmPublic by remember { mutableStateOf(false) }
    val performSend = {
        val s = staged
        if (s != null) {
            if (NodeController.sendTextWithAttachment(peer, text, s.name, s.bytes, s.mime)) {
                staged = null
                text = ""
            } else {
                confirm("Attachment too large for this node's store — send it smaller")
            }
        } else if (NodeController.send(peer, text)) {
            text = ""
        } else {
            confirm("Node not started yet — not sent")
        }
    }

    Column(Modifier.padding(horizontal = 12.dp).fillMaxSize().imePadding()) {
        if (peer != Petnames.PUBLIC) {
            val saved = names[peer] ?: ""
            // What `Petnames.set` will actually store. Compared rather than the
            // raw field, or trailing whitespace leaves the button lit forever
            // against a value that is already saved.
            val pending = editingName.trim()
            Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                ToughbookField(editingName, { editingName = it }, Modifier.weight(1f), placeholder = "petname")
                HGap()
                CrateButton(
                    "Save",
                    {
                        Petnames.set(peer, editingName)
                        confirm(if (pending.isEmpty()) "Petname cleared" else "Saved as “$pending”")
                    },
                    // Dimmed once the field matches what is stored, so the button
                    // carries state before it is pressed as well as after.
                    enabled = pending != saved,
                )
            }
        }
        // Collected once for the whole thread rather than per bubble: one
        // subscription, and every file row is guaranteed to show the same poll.
        val transfers by NodeController.transfers.collectAsState()
        val listState = rememberLazyListState()
        val scope = rememberCoroutineScope()
        val still = reducedMotion()
        // Whether the reader is already at (or near) the bottom. A new message
        // only pulls the view down automatically when they are — otherwise a
        // deliberate scroll into history is left alone, with the jump-to-bottom
        // button below as the manual way back (B7; this used to yank anyone
        // reading older messages straight back down on every new arrival).
        val atBottom by remember {
            derivedStateOf {
                val info = listState.layoutInfo
                val last = info.visibleItemsInfo.lastOrNull()?.index
                last == null || last >= info.totalItemsCount - 1
            }
        }
        LaunchedEffect(thread.size) {
            if (thread.isNotEmpty() && atBottom) listState.scrollToItem(thread.lastIndex)
        }
        Box(Modifier.weight(1f)) {
            LazyColumn(
                Modifier.fillMaxSize(), state = listState, verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                items(thread) { m ->
                    Bubble(m, m.magnet?.let { mg -> transfers.firstOrNull { it.magnet == mg } })
                }
            }
            if (!atBottom && thread.isNotEmpty()) {
                CrateButton(
                    "↓ new",
                    {
                        scope.launch {
                            if (still) listState.scrollToItem(thread.lastIndex)
                            else listState.animateScrollToItem(thread.lastIndex)
                        }
                    },
                    Modifier.align(Alignment.BottomEnd).padding(12.dp),
                    contentDescription = "Jump to latest message",
                )
            }
        }
        // The staged attachment, if any: a chip above the composer with a clear
        // affordance to drop it before sending.
        staged?.let { s ->
            Crate(Modifier.fillMaxWidth().padding(bottom = 6.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(if (s.mime.startsWith("image/")) "🖼" else "📎", Modifier.padding(end = 8.dp))
                    Column(Modifier.weight(1f)) {
                        Text(s.name, color = Palette.Ink, fontWeight = FontWeight.Bold, maxLines = 1)
                        Caption("${s.bytes.size / 1024} KB · staged, not sent")
                    }
                    CrateButton("✕", { staged = null }, contentDescription = "Remove staged attachment")
                }
            }
        }
        Row(Modifier.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            CrateButton("📎", { pickFile.launch("*/*") }, contentDescription = "Attach file")
            ToughbookField(
                text, { text = it }, Modifier.weight(1f).focusRequester(composerFocus), placeholder = "message…",
            )
            HGap()
            CrateButton(
                "Send",
                { if (peer == Petnames.PUBLIC) confirmPublic = true else performSend() },
                // Send is live with either text or an attachment.
                enabled = text.isNotBlank() || staged != null,
                face = Palette.Yellow,
                // Pink face, void ink: 5.58:1. The reverse (pink on olive) is the
                // one pairing §1 bans outright.
                ink = Palette.Paper,
            )
        }
    }

    if (confirmPublic) {
        ConfirmDialog(
            title = "Send to everyone?",
            body = "PUBLIC reaches every node in range, not just this conversation — " +
                "signed, but never sealed, so anyone carrying it can read it. This can't be unsent.",
            confirmLabel = "Send to PUBLIC",
            onConfirm = { confirmPublic = false; performSend() },
            onDismiss = { confirmPublic = false },
        )
    }
}

/** A file chosen in the composer, held until Send (not yet on the wire). */
internal data class StagedAttachment(val name: String, val bytes: ByteArray, val mime: String)

/**
 * A message as a crate. Sent and received are told apart by *border colour and
 * side*, not by fill — §1 forbids signalling by colour alone, and the alignment
 * is what carries it for anyone who cannot see the difference.
 */
@Composable
private fun Bubble(m: Msg, transfer: Transfer?) {
    val mine = m.mine
    // Strip the attachment marker from the displayed text; the attachment itself
    // renders as a preview/chip below rather than as a line of marker syntax.
    val shownText = remember(m.text) { Markdown.parseAttach(m.text).first }
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = if (mine) Arrangement.End else Arrangement.Start,
    ) {
        Box(Modifier.widthIn(max = 280.dp)) {
            Crate(edge = if (mine) Palette.Yellow else Palette.Ink) {
                Column {
                    if (!mine) Caption("${if (m.encrypted) "🔒 " else ""}${Petnames.label(m.peer)}")
                    if (shownText.isNotEmpty()) Text(shownText, color = Palette.Ink)
                    if (m.magnet != null) {
                        if (shownText.isNotEmpty()) VGap(6.dp)
                        Attachment(m.magnet, m.mime, m.text)
                    }
                    FragmentStatus(m, transfer)
                    Row(Modifier.padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                        Caption(timeOf(m.ts))
                        if (!m.mine && !m.verified) {
                            HGap(6.dp)
                            StickerBadge("⚠ signature BAD", ink = Palette.Yellow)
                        }
                        if (mine && m.id != null) {
                            HGap(6.dp)
                            if (m.delivered) StickerBadge("✓ delivered", ink = Palette.Ink)
                            else Caption("· sent")
                        }
                    }
                }
            }
        }
    }
}

/**
 * What became of this message on the wire: fountain chunks for a file, wire frames
 * for a long text. §3's segmented LED, because the unit of work is a countable
 * number of pieces and a smooth bar would be inventing precision.
 *
 * The two cases are labelled differently on purpose. A **file** is fountain-coded
 * into chunks any relay can carry, so `have/count` is real progress. Our *own*
 * file is complete the moment it is published — the LED fills immediately, and it
 * says "served from this node" rather than implying anyone has fetched it, because
 * whether a peer pulled a chunk is not something this node can observe. Claiming
 * delivery we cannot see is how a status line becomes a lie.
 */
@Composable
private fun FragmentStatus(m: Msg, transfer: Transfer?) {
    when {
        transfer != null -> {
            VGap(4.dp)
            Caption("micro-packing")
            SegmentedLed(transfer.have, transfer.count, Modifier.fillMaxWidth())
            Caption(
                if (transfer.have < transfer.count) "${transfer.have}/${transfer.count} chunks · fetching"
                else if (m.mine) "${transfer.count} chunks · served from this node"
                else "${transfer.count} chunks · complete"
            )
        }
        // A file whose manifest we have not polled yet, so there is no count to show.
        m.magnet != null -> {
            VGap(4.dp)
            Caption("micro-packing · counting chunks")
        }
        m.mine && m.fragments > 1 -> {
            VGap(4.dp)
            Caption("micro-packing")
            SegmentedLed(m.fragments, m.fragments, Modifier.fillMaxWidth())
            Caption("${m.fragments} wire frames")
        }
    }
}

/**
 * The attachment part of a bubble: an inline image preview when the file is an
 * image and its bytes are on disk, otherwise a tappable file chip. Tapping either
 * hands the file to another app via [openAttachment].
 *
 * `path` is null until the file is complete on this device (our own send caches it
 * immediately; a received file lands when [NodeController.pumpFiles] saves it). No
 * path means the chip reads as not-here-yet and does not offer Open — the LED in
 * [FragmentStatus] carries the progress.
 */
@Composable
private fun Attachment(magnet: String, mime: String?, body: String) {
    val ctx = LocalContext.current
    val paths by NodeController.filePaths.collectAsState()
    val path = paths[magnet]
    val att = remember(body) { Markdown.parseAttach(body).second }
    val name = att?.name ?: "attachment"
    val isImage = (mime ?: att?.mime).orEmpty().startsWith("image/")
    val here = path != null

    if (isImage && here) {
        val bmp by androidx.compose.runtime.produceState<android.graphics.Bitmap?>(null, path) {
            value = withContext(Dispatchers.IO) {
                runCatching {
                    val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
                    android.graphics.BitmapFactory.decodeFile(path, bounds)
                    var scale = 1
                    while (bounds.outWidth / scale > 1080) scale *= 2
                    android.graphics.BitmapFactory.decodeFile(
                        path,
                        android.graphics.BitmapFactory.Options().apply { inSampleSize = scale },
                    )
                }.getOrNull()
            }
        }
        val shown = bmp
        if (shown != null) {
            Image(
                shown.asImageBitmap(),
                contentDescription = "attached image: $name",
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(max = 220.dp)
                    .border(2.dp, Palette.Ink)
                    .clickable { openAttachment(ctx, magnet, name, mime ?: att?.mime, path) },
                contentScale = androidx.compose.ui.layout.ContentScale.Crop,
            )
            return
        }
    }

    // File chip (non-image, or image not decodable / not here yet).
    Row(
        Modifier
            .fillMaxWidth()
            .border(2.dp, Palette.Ink, CrateShape)
            .then(if (here) Modifier.clickable { openAttachment(ctx, magnet, name, mime ?: att?.mime, path) } else Modifier)
            .padding(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(if (isImage) "🖼" else "📎", Modifier.padding(end = 8.dp))
        Column(Modifier.weight(1f)) {
            Text(name, color = Palette.Ink, maxLines = 1)
            Caption(if (here) "tap to open" else "fetching…")
        }
    }
}

/**
 * Hand a completed attachment to another app.
 *
 * The bytes are copied into `cacheDir/attachments/<magnet>` (reclaimable, never
 * world-readable) and shared through a `FileProvider` content URI with a one-shot
 * read grant — never a `file://` path, which modern Android rejects and which would
 * expose the private store. `path` is the local copy when we have one; otherwise we
 * ask the core to open the file from its store.
 */
private fun openAttachment(ctx: android.content.Context, magnet: String, name: String, mime: String?, path: String?) {
    runCatching {
        val dir = File(ctx.cacheDir, "attachments/${magnet.take(16)}").apply { mkdirs() }
        val out = File(dir, safeName(name))
        if (!out.exists() || out.length() == 0L) {
            val bytes = path?.let { File(it).takeIf(File::exists)?.readBytes() }
                ?: NodeController.openAttachmentBytes(magnet)
                ?: return
            out.writeBytes(bytes)
        }
        val uri = androidx.core.content.FileProvider.getUriForFile(ctx, "${ctx.packageName}.files", out)
        val intent = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri, mime ?: "application/octet-stream")
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        ctx.startActivity(Intent.createChooser(intent, "Open $name"))
    }.onFailure { android.util.Log.w("spore", "could not open attachment", it) }
}

/** '/' and friends sanitised away so a sender's name can't escape the directory. */
private fun safeName(name: String): String =
    name.replace(Regex("[^A-Za-z0-9._-]"), "_").ifBlank { "file.bin" }
