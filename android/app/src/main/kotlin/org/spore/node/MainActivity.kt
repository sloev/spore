package org.spore.node

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The app's one snackbar host, reachable from any screen.
 *
 * Both Save buttons live several composables below the [Scaffold] that owns the
 * host, and neither had any way to say "done". The buttons were never broken —
 * they persisted on the first click all along — they just looked broken, which
 * is the same bug as far as anyone using the app is concerned.
 */
private val LocalSnackbar = staticCompositionLocalOf<SnackbarHostState> {
    error("no SnackbarHostState in scope — App() provides it")
}

/** Confirm an action that otherwise leaves no trace on screen. */
@Composable
private fun rememberConfirm(): (String) -> Unit {
    val host = LocalSnackbar.current
    val scope = rememberCoroutineScope()
    return { msg -> scope.launch { host.showSnackbar(msg) } }
}

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ContextCompat.startForegroundService(this, Intent(this, NodeService::class.java))
        setContent { App() }
    }
}

private sealed interface Screen
private data object Chats : Screen
private data class Chat(val peer: String) : Screen
private data object Feed : Screen
private data class Compose(val topic: String) : Screen
private data object BridgesScreen : Screen
private data object Advanced : Screen
private data object Connect : Screen

/** Wall-clock HH:mm for a message stamp. */
private fun timeOf(ts: Long): String =
    java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault()).format(java.util.Date(ts))

/** 🍄 with a brief sparkle whenever the node relays/receives (kawaii heartbeat). */
@Composable
private fun mascot(): String {
    val tick by NodeController.relayTick.collectAsState()
    var sparkle by remember { mutableStateOf(false) }
    // The sparkle is an animation, so it does not happen at all under reduced
    // motion — §0 constraint 2 asks for completely static, not subtler.
    val still = reducedMotion()
    LaunchedEffect(tick) {
        if (tick != 0L && !still) { sparkle = true; delay(1500); sparkle = false }
    }
    return if (sparkle) "🍄✨" else "🍄"
}

@Composable
fun App() {
    val dark = androidx.compose.foundation.isSystemInDarkTheme()
    MaterialTheme(
        colorScheme = if (dark) SporeDarkColors else SporeLightColors,
        typography = SporeTypography,
    ) {
        val ctx = LocalContext.current
        val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {}
        LaunchedEffect(Unit) {
            if (Build.VERSION.SDK_INT >= 33 &&
                ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
            ) ask.launch(Manifest.permission.POST_NOTIFICATIONS)
        }

        var screen by remember { mutableStateOf<Screen>(Chats) }
        val addr by NodeController.address.collectAsState()
        val snackbar = remember { SnackbarHostState() }
        // Scanlines only in the dark theme: Field Notes is the printed-manual
        // voice and a CRT artefact on paper is nonsense (§1 light mode).
        val scan = dark && !reducedMotion()

        Scaffold(
            snackbarHost = { SnackbarHost(snackbar) },
            topBar = { TopBar(screen) { screen = it } },
            bottomBar = {
                if (screen.showsNav()) BottomNav(screen) { screen = it }
            },
            containerColor = MaterialTheme.colorScheme.background,
        ) { pad ->
            CompositionLocalProvider(LocalSnackbar provides snackbar) {
                Column(Modifier.padding(pad).fillMaxSize().scanlines(scan)) {
                    ReceivingBar()
                    TransfersBar()
                    when (val s = screen) {
                        Chats -> ChatsList(addr) { screen = Chat(it) }
                        is Chat -> ChatDetail(s.peer)
                        Feed -> FeedScreen { screen = Compose(it) }
                        is Compose -> ComposePost(s.topic) { screen = Feed }
                        BridgesScreen -> BridgesList()
                        Advanced -> AdvancedScreen(addr)
                        Connect -> ConnectScreen()
                    }
                }
            }
        }
    }
}

private fun Screen.showsNav(): Boolean =
    this == Chats || this == Feed || this == BridgesScreen

private fun Screen.title(): String = when (this) {
    is Chat -> Petnames.label(peer)
    Feed -> "Feed"
    is Compose -> "New post"
    BridgesScreen -> "Bridges"
    Advanced -> "Advanced"
    Connect -> "Connect"
    else -> "SPORE"
}

/**
 * The app bar as a crate lid: display heading, a status line under it, and the
 * peer count as a sticker on the right — the mock's shape, with §3's chrome.
 */
@Composable
private fun TopBar(screen: Screen, go: (Screen) -> Unit) {
    val m = mascot()
    val peers by NodeController.peers.collectAsState()
    val stored by NodeController.storeCount.collectAsState()

    Row(
        Modifier
            .fillMaxWidth()
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .padding(start = 12.dp, end = 12.dp, top = 10.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (!screen.showsNav()) {
            Text(
                "←",
                Modifier
                    .clickable { go(if (screen is Compose) Feed else Chats) }
                    .padding(end = 10.dp),
                color = Palette.Amber,
            )
        }
        Column(Modifier.weight(1f)) {
            DisplayHeading("$m ${screen.title()}")
            // In a thread, say whether we can actually seal to this peer — it is
            // the one status that changes what a message means. Elsewhere, what
            // this node is carrying for everyone else.
            val sub = if (screen is Chat && screen.peer != Petnames.PUBLIC) {
                if (peers.any { it.addr == screen.peer && it.hasKey }) "🔒 sealed to their prekey"
                else "signed, not yet sealable"
            } else {
                "$stored envelopes held for the mesh"
            }
            Caption(sub)
        }
        if (screen.showsNav()) {
            StickerBadge("${peers.size} peers", ink = Palette.Phosphor)
            HGap(6.dp)
            Text("👋", Modifier.clickable { go(Connect) }.padding(4.dp))
            Text("⚙", Modifier.clickable { go(Advanced) }.padding(4.dp))
        }
    }
}

/** Bottom nav: a lit block over an uppercase label, per the mock. */
@Composable
private fun BottomNav(screen: Screen, go: (Screen) -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .padding(vertical = 8.dp),
    ) {
        listOf<Pair<String, Screen>>("Chats" to Chats, "Feed" to Feed, "Bridges" to BridgesScreen)
            .forEach { (label, target) ->
                val on = screen == target
                Column(
                    Modifier.weight(1f).clickable { go(target) },
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Box(
                        Modifier
                            .size(14.dp)
                            .then(
                                if (on) Modifier.background(Palette.Pink)
                                else Modifier.border(1.dp, Palette.Dim)
                            )
                    )
                    VGap(3.dp)
                    Caption(label.uppercase(), color = if (on) Palette.Pink else Palette.Dim)
                }
            }
    }
}

/** Files still arriving, with chunk progress as a segmented LED. */
@Composable
private fun TransfersBar() {
    val xs by NodeController.transfers.collectAsState()
    val active = xs.filter { it.have < it.count }
    if (active.isEmpty()) return
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp)) {
        active.take(3).forEach { t ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                Caption("📎 ${t.name}", Modifier.weight(1f))
                HGap()
                SegmentedLed(t.have, t.count, Modifier.width(80.dp))
                HGap(6.dp)
                Caption("${t.have}/${t.count}")
            }
        }
    }
}

/** Live receive-side fragmentation status ("receiving X/N"). */
@Composable
private fun ReceivingBar() {
    val recv by NodeController.receiving.collectAsState()
    if (recv.isBlank()) return
    val lines = recv.lines().filter { it.isNotBlank() }
    Caption(
        lines.joinToString("  ·  ") { "⇣ receiving ${it.substringAfter(':', "?")}" },
        Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 2.dp),
    )
}

@Composable
private fun ChatsList(addr: String, open: (String) -> Unit) {
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
private fun ChatDetail(peer: String) {
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

@Composable
private fun FeedScreen(compose: (String) -> Unit) {
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
        Modifier.clickable(onClick = onClick).padding(vertical = 2.dp),
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
private fun ComposePost(topic: String, done: () -> Unit) {
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
            CrateButton("B", { body = Markdown.wrap(body, "**") })
            CrateButton("i", { body = Markdown.wrap(body, "_") })
            CrateButton("</>", { body = Markdown.wrap(body, "`") })
            CrateButton("🔗", { body = Markdown.link(body) })
            CrateButton("🖼", { pickImage.launch("image/*") })
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
                        NodeController.post(target, body.text); true
                    } else {
                        NodeController.postWithImage(target, body.text, img.first, img.second)
                    }
                    if (ok) { confirm("Posted to #$target"); done() }
                    else confirm("Image too large for this node's store — post it smaller")
                },
                enabled = body.text.isNotBlank() || image != null,
                face = Palette.Pink,
                ink = Palette.Void,
            )
        }
    }
}

/**
 * Meet someone: show them your QR, or scan/paste theirs. An invite carries your
 * address, the name you announce, and the bridges you're reachable on — so they
 * can join the same mesh, not merely learn a number.
 */
@Composable
private fun ConnectScreen() {
    val ctx = LocalContext.current
    val myName by NodeController.myName.collectAsState()
    val invite = remember(myName) { NodeController.inviteText() }

    var scanning by remember { mutableStateOf(false) }
    var pasted by remember { mutableStateOf("") }
    var found by remember { mutableStateOf<ScannedInvite?>(null) }
    var petname by remember { mutableStateOf("") }
    var chosen by remember { mutableStateOf(setOf<String>()) }
    var note by remember { mutableStateOf("") }

    val camPerm = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        scanning = ok
        if (!ok) note = "camera denied — paste the invite text instead"
    }

    fun accept(inv: ScannedInvite) {
        found = inv
        petname = inv.suggestedName
        chosen = emptySet()
        scanning = false
    }

    LazyColumn(Modifier.padding(12.dp).fillMaxSize(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        val pending = found
        if (pending == null) {
            item {
                Crate(Modifier.fillMaxWidth()) {
                    Column {
                        DisplayHeading("Your invite", size = 15)
                        Caption("Let a friend scan this. Shares your address, name, and relay bridges.")
                        VGap()
                        if (invite.isNotBlank()) QrImage(invite)
                        Caption(invite)
                        Row(Modifier.padding(top = 8.dp)) {
                            CrateButton("Copy", {
                                val cm = ctx.getSystemService(android.content.ClipboardManager::class.java)
                                cm?.setPrimaryClip(android.content.ClipData.newPlainText("SPORE invite", invite))
                            })
                            CrateButton("Share", {
                                val i = Intent(Intent.ACTION_SEND).apply {
                                    type = "text/plain"; putExtra(Intent.EXTRA_TEXT, invite)
                                }
                                ctx.startActivity(Intent.createChooser(i, "Share your SPORE invite"))
                            })
                        }
                    }
                }
            }
            item {
                Crate(Modifier.fillMaxWidth()) {
                    Column {
                        DisplayHeading("Add a friend", size = 15)
                        VGap()
                        if (scanning) {
                            QrScanner(onResult = { text ->
                                val inv = NodeController.parseInvite(text)
                                if (inv != null) accept(inv) else note = "that QR isn't a SPORE invite"
                            })
                            CrateButton("Stop scanning", { scanning = false })
                        } else {
                            CrateButton("Scan their QR", {
                                if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.CAMERA)
                                    == PackageManager.PERMISSION_GRANTED
                                ) scanning = true else camPerm.launch(Manifest.permission.CAMERA)
                            })
                        }
                        VGap()
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            ToughbookField(pasted, { pasted = it }, Modifier.weight(1f), placeholder = "…or paste an invite")
                            HGap()
                            CrateButton("Add", {
                                val inv = NodeController.parseInvite(pasted)
                                if (inv != null) { accept(inv); pasted = "" }
                                else note = "that doesn't look like a valid invite"
                            })
                        }
                        if (note.isNotBlank()) { VGap(4.dp); Caption(note) }
                    }
                }
            }
        } else {
            item {
                Crate(Modifier.fillMaxWidth()) {
                    Column {
                        DisplayHeading("Add ${pending.suggestedName.ifBlank { "this node" }}?", size = 15)
                        Caption("address ${pending.addr}")
                        Caption("The name in an invite is only what they claim — the petname you set here is the one you'll trust.")
                        VGap()
                        ToughbookField(petname, { petname = it }, Modifier.fillMaxWidth(), placeholder = "petname")
                    }
                }
            }
            if (pending.bridges.isNotEmpty()) {
                item {
                    Crate(Modifier.fillMaxWidth(), edge = Palette.Pink) {
                        Column {
                            DisplayHeading("Also join their bridges?", size = 15, color = Palette.Pink)
                            Caption("Only tick these if you trust the sender: joining connects your node to servers they chose.")
                            pending.bridges.forEach { b ->
                                Row(
                                    Modifier.fillMaxWidth().padding(top = 6.dp).clickable {
                                        chosen = if (b in chosen) chosen - b else chosen + b
                                    },
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Text(if (b in chosen) "☑" else "☐", color = Palette.Amber)
                                    HGap()
                                    Caption(b)
                                }
                            }
                        }
                    }
                }
            }
            item {
                Row {
                    CrateButton("Add contact", {
                        NodeController.acceptInvite(pending, petname)
                        if (chosen.isNotEmpty()) NodeController.applyInviteBridges(ctx, chosen.toList())
                        note = "added ${petname.ifBlank { pending.suggestedName }}"
                        found = null
                    }, face = Palette.Pink, ink = Palette.Void)
                    CrateButton("Cancel", { found = null })
                }
            }
        }
    }
}

@Composable
private fun AdvancedScreen(addr: String) {
    val topics by NodeController.topics.collectAsState()
    var showSeed by remember { mutableStateOf(false) }
    val confirm = rememberConfirm()
    val myName by NodeController.myName.collectAsState()
    var editName by remember(myName) { mutableStateOf(myName) }
    val nearby by NodeController.peers.collectAsState()
    val stored by NodeController.storeCount.collectAsState()
    val resumed by NodeController.resumed.collectAsState()

    LazyColumn(
        Modifier.padding(12.dp).fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        item {
            Crate(Modifier.fillMaxWidth()) {
                Column {
                    DisplayHeading("You", size = 15)
                    Caption("Announced to nodes in range as a suggested petname. A hint, not an identity.")
                    VGap()
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        ToughbookField(editName, { editName = it }, Modifier.weight(1f), placeholder = "name")
                        HGap()
                        // `setMyName` trims and caps at 32; compare what it will
                        // store, not what is typed, or an over-long name never settles.
                        val pending = editName.trim().take(32)
                        CrateButton(
                            "Save",
                            {
                                val saved = NodeController.setMyName(editName)
                                confirm(
                                    when {
                                        !saved -> "Node not started yet — not saved"
                                        pending.isEmpty() -> "Name cleared"
                                        else -> "Announcing as “$pending”"
                                    }
                                )
                            },
                            enabled = pending != myName,
                        )
                    }
                    VGap()
                    Caption("address $addr")
                }
            }
        }

        item {
            Crate(Modifier.fillMaxWidth()) {
                Column {
                    DisplayHeading("Topics", size = 15)
                    VGap(6.dp)
                    if (topics.isEmpty()) Caption("none followed yet — subscribe from the Feed")
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        topics.take(6).forEach { StickerBadge("#$it", bg = Palette.Void) }
                    }
                }
            }
        }

        item {
            Crate(Modifier.fillMaxWidth()) {
                Column {
                    DisplayHeading("Node", size = 15)
                    Caption("peers heard: ${nearby.size} · can seal to ${nearby.count { it.hasKey }}")
                    Caption("envelopes relayed/stored: $stored")
                    if (resumed > 0) Caption("picked up $resumed from storage on start — transfers resume rather than restart 🌱")
                    VGap(6.dp)
                    Caption("store budget")
                    SegmentedLed(stored.coerceAtMost(500), 500, Modifier.fillMaxWidth(), height = 10.dp)
                }
            }
        }

        item {
            Crate(Modifier.fillMaxWidth()) {
                Column {
                    DisplayHeading("Seed", size = 15)
                    Caption("Anyone holding these 32 bytes IS this node. Only reveal to move your identity to another device.")
                    VGap()
                    if (!showSeed) {
                        CrateButton("Reveal seed", { showSeed = true })
                    } else {
                        // Read through NodeController: the seed moved into the
                        // encrypted store and reading the old plaintext file here
                        // showed "unavailable" on every upgraded install.
                        val seedHex = remember { NodeController.seedHex() ?: "unavailable" }
                        Text(seedHex, color = Palette.Amber)
                        VGap(4.dp)
                        CrateButton("Hide", { showSeed = false })
                    }
                }
            }
        }

        item {
            Crate(Modifier.fillMaxWidth()) {
                Column {
                    DisplayHeading("About", size = 15)
                    Caption("SPORE — store-and-forward planetary opportunistic relay envelope. This phone is a full node: it signs, relays, and delivers across every enabled bridge. Public domain. 🍄")
                }
            }
        }
    }
}

@Composable
private fun BridgesList() {
    val ctx = LocalContext.current
    val bridges by NodeController.bridges.collectAsState()
    var tcp by remember { mutableStateOf("") }
    var pendingAction by remember { mutableStateOf<(() -> Unit)?>(null) }
    val askPerms = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { granted ->
        if (granted.values.all { it }) pendingAction?.invoke()
        pendingAction = null
    }

    fun withPerms(perms: List<String>, action: () -> Unit) {
        val missing = perms.filter {
            ContextCompat.checkSelfPermission(ctx, it) != PackageManager.PERMISSION_GRANTED
        }
        if (missing.isEmpty()) action()
        else { pendingAction = action; askPerms.launch(missing.toTypedArray()) }
    }

    fun bonded(): List<android.bluetooth.BluetoothDevice> = try {
        val bt = ctx.getSystemService(android.bluetooth.BluetoothManager::class.java)
        bt?.adapter?.bondedDevices?.toList() ?: emptyList()
    } catch (_: SecurityException) { emptyList() }

    val blePerms = if (Build.VERSION.SDK_INT >= 31) listOf(Manifest.permission.BLUETOOTH_CONNECT) else emptyList()
    val wifiP2pPerms =
        if (Build.VERSION.SDK_INT >= 33) listOf(Manifest.permission.NEARBY_WIFI_DEVICES)
        else listOf(Manifest.permission.ACCESS_FINE_LOCATION)

    var showMeshPick by remember { mutableStateOf(false) }
    var showRnodePick by remember { mutableStateOf(false) }
    var freq by remember { mutableStateOf("867.2") }
    var bw by remember { mutableStateOf("125") }
    var sf by remember { mutableStateOf("8") }
    var cr by remember { mutableStateOf("5") }
    var tx by remember { mutableStateOf("0") }

    LazyColumn(Modifier.padding(horizontal = 12.dp).fillMaxSize()) {
        item {
            Caption(
                "Bridges relay your signed envelopes across every medium at once. 🍄",
                Modifier.padding(vertical = 8.dp),
            )
        }
        // Grouped the way the mock groups them. The kind string is what the core
        // reports, so the buckets are derived rather than stored — a new bridge
        // kind lands in "Other" instead of vanishing.
        val groups = bridges.groupBy { b ->
            val k = b.kind.lowercase()
            when {
                "ble" in k || "mesh" in k || "rnode" in k || "audio" in k || "wifi" in k || "nfc" in k -> "Radio"
                "tcp" in k || "udp" in k -> "Network"
                "web" in k || "nostr" in k || "ws" in k -> "Web"
                else -> "Other"
            }
        }
        listOf("Radio", "Network", "Web", "Other").forEach { name ->
            val rows = groups[name].orEmpty()
            if (rows.isNotEmpty()) {
                item { SectionLabel(name) }
                items(rows) { b -> BridgeRow(b) }
            }
        }

        item { SectionLabel("Add a bridge") }
        item {
            Row(Modifier.padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                ToughbookField(tcp, { tcp = it }, Modifier.weight(1f), placeholder = "TCP host:port (blank = listen)")
                HGap()
                CrateButton("Add", { NodeController.addTcp(tcp.trim()); tcp = "" })
            }
        }
        item {
            CrateButton("Audio modem", {
                withPerms(listOf(Manifest.permission.RECORD_AUDIO)) { NodeController.enableAudio() }
            }, Modifier.padding(vertical = 4.dp))
        }
        item {
            CrateButton("Meshtastic (paired BLE)", {
                withPerms(blePerms) { showMeshPick = !showMeshPick }
            }, Modifier.padding(vertical = 4.dp))
        }
        if (showMeshPick) {
            items(bonded()) { d ->
                Crate(Modifier.fillMaxWidth().clickable {
                    NodeController.enableMeshtasticBle(ctx, d); showMeshPick = false
                }) { Text("📻 ${try { d.name } catch (_: SecurityException) { null } ?: d.address}", color = Palette.Amber) }
            }
        }
        item {
            CrateButton("Reticulum RNode (paired BLE)", {
                withPerms(blePerms) { showRnodePick = !showRnodePick }
            }, Modifier.padding(vertical = 4.dp))
        }
        if (showRnodePick) {
            item {
                Row(Modifier.padding(vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    ToughbookField(freq, { freq = it }, Modifier.weight(1f), placeholder = "MHz")
                    ToughbookField(bw, { bw = it }, Modifier.weight(1f), placeholder = "kHz")
                    ToughbookField(sf, { sf = it }, Modifier.weight(0.7f), placeholder = "SF")
                    ToughbookField(cr, { cr = it }, Modifier.weight(0.7f), placeholder = "CR")
                    ToughbookField(tx, { tx = it }, Modifier.weight(0.7f), placeholder = "dBm")
                }
            }
            items(bonded()) { d ->
                Crate(Modifier.fillMaxWidth().clickable {
                    val f = ((freq.toDoubleOrNull() ?: 867.2) * 1e6).toLong()
                    val b = ((bw.toDoubleOrNull() ?: 125.0) * 1e3).toLong()
                    NodeController.enableRNodeBle(
                        ctx, d, f, b, sf.toIntOrNull() ?: 8, cr.toIntOrNull() ?: 5, tx.toIntOrNull() ?: 0
                    )
                    showRnodePick = false
                }) { Text("📡 ${try { d.name } catch (_: SecurityException) { null } ?: d.address}", color = Palette.Amber) }
            }
        }
        item {
            CrateButton("Wi-Fi Direct group", {
                withPerms(wifiP2pPerms) { NodeController.enableWifiDirect(ctx) }
            }, Modifier.padding(vertical = 4.dp))
        }
        item {
            var ws by remember { mutableStateOf("") }
            Row(Modifier.padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                ToughbookField(ws, { ws = it }, Modifier.weight(1f), placeholder = "WebSocket relay wss://…")
                HGap()
                CrateButton("Add", { NodeController.addWebSocket(ctx, ws); ws = "" })
            }
        }
        item {
            var nostr by remember { mutableStateOf("") }
            Row(Modifier.padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                ToughbookField(nostr, { nostr = it }, Modifier.weight(1f), placeholder = "Nostr relay wss://… (rx-only)")
                HGap()
                CrateButton("Add", { NodeController.addNostr(ctx, nostr); nostr = "" })
            }
        }
        item {
            var swarm by remember { mutableStateOf("spore/public") }
            Row(Modifier.padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                ToughbookField(swarm, { swarm = it }, Modifier.weight(1f), placeholder = "WebTorrent swarm name")
                HGap()
                CrateButton("Join", { NodeController.addWebTorrent(ctx, swarm) })
            }
        }
    }
}

/**
 * One bridge: an LED dot, the kind, its status line.
 *
 * The mock draws a switch here. There is no `nativeStopBridge` on the JNI side —
 * the bridge list is append-only by construction — so a switch would be a control
 * that cannot turn anything off. The dot reports state honestly instead, and the
 * toggle lands when the JNI call exists (docs/ANDROID_AUDIT.md §2).
 */
@Composable
private fun BridgeRow(b: BridgeState) {
    val s = b.status.lowercase()
    val (dot, label) = when {
        "up" in s || "connected" in s || "listening" in s || "ok" in s -> Palette.Phosphor to b.status
        "connect" in s || "start" in s -> Palette.Amber to b.status
        else -> Palette.Kevlar to b.status
    }
    Crate(Modifier.fillMaxWidth()) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(b.kind, color = Palette.Amber, fontWeight = FontWeight.Bold)
                Caption(b.detail)
            }
            HGap()
            LedDot(dot)
            HGap(6.dp)
            // Never colour alone (§1) — the dot is paired with the status word.
            Caption(label)
        }
    }
}
