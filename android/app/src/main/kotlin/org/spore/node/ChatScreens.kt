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
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

@Composable
internal fun ChatsList(addr: String, open: (String) -> Unit) {
    val ctx = LocalContext.current
    val messages by NodeController.messages.collectAsState()
    val names by Petnames.map.collectAsState()
    val nearby by NodeController.peers.collectAsState()
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
                    Text(addr, color = Palette.Amber)
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
                            Avatar(shown)
                            HGap(10.dp)
                            Column(Modifier.weight(1f)) {
                                Text(shown, color = Palette.Amber, fontWeight = FontWeight.Bold)
                                Caption("📡 $ago")
                            }
                            if (p.hasKey) StickerBadge("🔒 sealed", ink = Palette.Phosphor)
                        }
                    }
                }
            } else {
                item {
                    Caption(
                        "no spores nearby yet 🍄\nadd a bridge, and anyone in range appears here",
                        Modifier.fillMaxWidth().padding(24.dp),
                    )
                }
            }

            item { SectionLabel("Conversations") }
            items(threads) { peer ->
                val last = messages.lastOrNull { it.peer == peer }
                // The mock shows an unread count here. There is no read tracking in
                // the app, so a badge would be a number with nothing behind it —
                // left out rather than faked. Time and last line carry the row.
                Crate(Modifier.fillMaxWidth().clickable { open(peer) }) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Avatar(Petnames.label(peer))
                        HGap(10.dp)
                        Column(Modifier.weight(1f)) {
                            Row {
                                Text(
                                    Petnames.label(peer),
                                    Modifier.weight(1f),
                                    color = Palette.Amber,
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
                        if (unread > 0) {
                            HGap(6.dp)
                            StickerBadge("$unread", ink = Palette.Pink)
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

/** Initial in a kevlar tile — the mock's peer avatar. */
@Composable
private fun Avatar(name: String, size: Int = 34) {
    Box(
        Modifier
            .size(size.dp)
            .background(Palette.Kevlar, CrateShape)
            .border(2.dp, Palette.Edge, CrateShape),
        contentAlignment = Alignment.Center,
    ) {
        // Amber on kevlar is 4.48:1 — large text only (§1), which a bold initial is.
        Text(
            name.firstOrNull()?.uppercase() ?: "?",
            color = Palette.Amber,
            fontWeight = FontWeight.Bold,
        )
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

    val pickFile = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri: Uri? ->
        if (uri != null) {
            val name = ctx.contentResolver.query(uri, null, null, null, null)?.use { c ->
                val i = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (c.moveToFirst() && i >= 0) c.getString(i) else null
            } ?: "file.bin"
            val data = ctx.contentResolver.openInputStream(uri)?.use { it.readBytes() }
            if (data != null) NodeController.sendFile(peer, name, data)
        }
    }

    Column(Modifier.padding(horizontal = 12.dp).fillMaxSize()) {
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
        LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            items(thread) { m ->
                Bubble(m, m.magnet?.let { mg -> transfers.firstOrNull { it.magnet == mg } })
            }
        }
        Row(Modifier.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            CrateButton("📎", { pickFile.launch("*/*") })
            ToughbookField(text, { text = it }, Modifier.weight(1f), placeholder = "message…")
            HGap()
            CrateButton(
                "Send",
                { NodeController.send(peer, text); text = "" },
                enabled = text.isNotBlank(),
                face = Palette.Pink,
                // Pink face, void ink: 5.58:1. The reverse (pink on olive) is the
                // one pairing §1 bans outright.
                ink = Palette.Void,
            )
        }
    }
}

/**
 * A message as a crate. Sent and received are told apart by *border colour and
 * side*, not by fill — §1 forbids signalling by colour alone, and the alignment
 * is what carries it for anyone who cannot see the difference.
 */
@Composable
private fun Bubble(m: Msg, transfer: Transfer?) {
    val mine = m.mine
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = if (mine) Arrangement.End else Arrangement.Start,
    ) {
        Box(Modifier.widthIn(max = 280.dp)) {
            Crate(edge = if (mine) Palette.Pink else Palette.Edge) {
                Column {
                    if (!mine) Caption("${if (m.encrypted) "🔒 " else ""}${Petnames.label(m.peer)}")
                    Text(m.text, color = Palette.Amber)
                    FragmentStatus(m, transfer)
                    Row(Modifier.padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                        Caption(timeOf(m.ts))
                        if (!m.mine && !m.verified) {
                            HGap(6.dp)
                            StickerBadge("⚠ signature BAD", ink = Palette.Pink)
                        }
                        if (mine && m.id != null) {
                            HGap(6.dp)
                            if (m.delivered) StickerBadge("✓ delivered", ink = Palette.Phosphor)
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
