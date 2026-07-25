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
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.delay

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
private data object BridgesScreen : Screen
private data object Advanced : Screen
private data object Connect : Screen

// Kawaii-but-serious: Meshtastic-adjacent greens with a soft pastel accent.
private val SporeLightColors = androidx.compose.material3.lightColorScheme(
    primary = androidx.compose.ui.graphics.Color(0xFF2E7D4F),
    secondary = androidx.compose.ui.graphics.Color(0xFF57C785),
    tertiary = androidx.compose.ui.graphics.Color(0xFFF2A6C9),
    surfaceVariant = androidx.compose.ui.graphics.Color(0xFFE8F4EC),
)
private val SporeDarkColors = androidx.compose.material3.darkColorScheme(
    primary = androidx.compose.ui.graphics.Color(0xFF57C785),
    secondary = androidx.compose.ui.graphics.Color(0xFF7ADBA2),
    tertiary = androidx.compose.ui.graphics.Color(0xFFF2A6C9),
)

/** Wall-clock HH:mm for a message stamp. */
private fun timeOf(ts: Long): String =
    java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault()).format(java.util.Date(ts))

/** 🍄 with a brief sparkle whenever the node relays/receives (kawaii heartbeat). */
@Composable
private fun mascot(): String {
    val tick by NodeController.relayTick.collectAsState()
    var sparkle by remember { mutableStateOf(false) }
    LaunchedEffect(tick) {
        if (tick != 0L) { sparkle = true; delay(1500); sparkle = false }
    }
    return if (sparkle) "🍄✨" else "🍄"
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun App() {
    val dark = androidx.compose.foundation.isSystemInDarkTheme()
    MaterialTheme(colorScheme = if (dark) SporeDarkColors else SporeLightColors) {
        val ctx = LocalContext.current
        val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {}
        LaunchedEffect(Unit) {
            if (Build.VERSION.SDK_INT >= 33 &&
                ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
            ) ask.launch(Manifest.permission.POST_NOTIFICATIONS)
        }

        var screen by remember { mutableStateOf<Screen>(Chats) }
        val addr by NodeController.address.collectAsState()
        val m = mascot()

        Scaffold(
            topBar = {
                TopAppBar(title = {
                    when (val s = screen) {
                        is Chat -> Text("$m ${Petnames.label(s.peer)}")
                        Feed -> Text("$m Feed")
                        BridgesScreen -> Text("$m Bridges")
                        Advanced -> Text("$m Advanced")
                        Connect -> Text("$m Connect")
                        else -> Text("$m SPORE")
                    }
                }, navigationIcon = {
                    if (screen is Chat || screen == Advanced || screen == Connect) {
                        IconButton(onClick = { screen = Chats }) { Text("←") }
                    }
                }, actions = {
                    if (screen !is Chat && screen != Advanced && screen != Connect) {
                        IconButton(onClick = { screen = Connect }) { Text("👋") }
                        IconButton(onClick = { screen = Advanced }) { Text("⚙") }
                    }
                })
            },
            bottomBar = {
                if (screen !is Chat && screen != Advanced && screen != Connect) {
                    NavigationBar {
                        NavigationBarItem(selected = screen == Chats, onClick = { screen = Chats }, icon = {}, label = { Text("Chats") })
                        NavigationBarItem(selected = screen == Feed, onClick = { screen = Feed }, icon = {}, label = { Text("Feed") })
                        NavigationBarItem(selected = screen == BridgesScreen, onClick = { screen = BridgesScreen }, icon = {}, label = { Text("Bridges") })
                    }
                }
            }
        ) { pad ->
            Column(Modifier.padding(pad).fillMaxSize()) {
                ReceivingBar()
                TransfersBar()
                when (val s = screen) {
                    Chats -> ChatsList(addr) { screen = Chat(it) }
                    is Chat -> ChatDetail(s.peer)
                    Feed -> FeedScreen()
                    BridgesScreen -> BridgesList()
                    Advanced -> AdvancedScreen(addr)
                    Connect -> ConnectScreen()
                }
            }
        }
    }
}

/** Files still arriving, with chunk progress. */
@Composable
private fun TransfersBar() {
    val xs by NodeController.transfers.collectAsState()
    val active = xs.filter { it.have < it.count }
    if (active.isNotEmpty()) {
        Text(
            active.joinToString("  ·  ") { "📎 ${it.have}/${it.count} chunks" },
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 2.dp),
            style = MaterialTheme.typography.bodySmall
        )
    }
}

/** Live receive-side fragmentation status ("receiving X/N"). */
@Composable
private fun ReceivingBar() {
    val recv by NodeController.receiving.collectAsState()
    if (recv.isNotBlank()) {
        val lines = recv.lines().filter { it.isNotBlank() }
        val label = lines.joinToString("  ·  ") { l ->
            val p = l.substringAfter(':', "?")
            "⇣ receiving $p"
        }
        Text(
            label,
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 2.dp),
            style = MaterialTheme.typography.bodySmall
        )
    }
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
    Column(Modifier.padding(16.dp).fillMaxSize()) {
        // Your address is what others need to reach you — make it one tap to hand over.
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("you: $addr", style = MaterialTheme.typography.bodySmall, modifier = Modifier.weight(1f))
            OutlinedButton(onClick = {
                val cm = ctx.getSystemService(android.content.ClipboardManager::class.java)
                cm?.setPrimaryClip(android.content.ClipData.newPlainText("SPORE address", addr))
            }) { Text("Copy") }
            Spacer(Modifier.width(4.dp))
            OutlinedButton(onClick = {
                val i = Intent(Intent.ACTION_SEND).apply {
                    type = "text/plain"
                    putExtra(Intent.EXTRA_TEXT, "My SPORE address: $addr")
                }
                ctx.startActivity(Intent.createChooser(i, "Share your SPORE address"))
            }) { Text("Share") }
        }
        Spacer(Modifier.height(8.dp))

        LazyColumn(Modifier.weight(1f)) {
            // Who's actually out there right now — the mesh view a radio user expects.
            if (nearby.isNotEmpty()) {
                item {
                    Text("Nearby (${nearby.size})", style = MaterialTheme.typography.titleSmall)
                }
                items(nearby) { p ->
                    val ago = if (p.secondsAgo < 60) "${p.secondsAgo}s ago" else "${p.secondsAgo / 60}m ago"
                    // Your petname wins; otherwise show what they call themselves,
                    // quoted so it reads as a claim rather than a verified name.
                    val shown = names[p.addr] ?: p.announced.takeIf { it.isNotBlank() }?.let { "“$it”" }
                        ?: Petnames.label(p.addr)
                    Card(Modifier.fillMaxWidth().padding(vertical = 3.dp).clickable { open(p.addr) }) {
                        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                "📡 $shown",
                                style = MaterialTheme.typography.titleSmall,
                                modifier = Modifier.weight(1f)
                            )
                            Text(
                                (if (p.hasKey) "🔒 " else "") + ago,
                                style = MaterialTheme.typography.bodySmall
                            )
                        }
                    }
                }
                item { Spacer(Modifier.height(12.dp)) }
            } else {
                item {
                    Text(
                        "no spores nearby yet 🍄\nadd a bridge, and anyone in range appears here",
                        Modifier.fillMaxWidth().padding(24.dp), textAlign = TextAlign.Center,
                        style = MaterialTheme.typography.bodySmall
                    )
                }
            }

            item { Text("Conversations", style = MaterialTheme.typography.titleSmall) }
            items(threads) { peer ->
                val last = messages.lastOrNull { it.peer == peer }
                Card(Modifier.fillMaxWidth().padding(vertical = 4.dp).clickable { open(peer) }) {
                    Column(Modifier.padding(12.dp)) {
                        Text(Petnames.label(peer), style = MaterialTheme.typography.titleMedium)
                        Text(
                            (if (last?.encrypted == true) "🔒 " else "") + (last?.text ?: "—"),
                            style = MaterialTheme.typography.bodySmall, maxLines = 1
                        )
                    }
                }
            }
            item {
                Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(
                        value = newPeer, onValueChange = { newPeer = it },
                        label = { Text("open chat by address (16 hex)") }, modifier = Modifier.weight(1f)
                    )
                    Spacer(Modifier.width(8.dp))
                    OutlinedButton(onClick = {
                        val h = newPeer.trim().lowercase()
                        if (h.length == 16) { open(h); newPeer = "" }
                    }) { Text("Open") }
                }
            }
        }
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

    Column(Modifier.padding(16.dp).fillMaxSize()) {
        if (peer != Petnames.PUBLIC) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = editingName, onValueChange = { editingName = it },
                    label = { Text("petname") }, modifier = Modifier.weight(1f)
                )
                Spacer(Modifier.width(8.dp))
                OutlinedButton(onClick = { Petnames.set(peer, editingName) }) { Text("Save") }
            }
            Spacer(Modifier.height(8.dp))
        }
        LazyColumn(Modifier.weight(1f)) {
            items(thread) { m ->
                val who = if (m.mine) "you" else Petnames.label(m.peer)
                val lock = if (m.encrypted) "🔒 " else ""
                Column(Modifier.padding(vertical = 3.dp)) {
                    Text("${if (m.mine) "▶" else "◀"} $lock$who: ${m.text}")
                    // One quiet metadata line: when, signature, delivery, fragments.
                    val bits = buildList {
                        add(timeOf(m.ts))
                        if (!m.mine && !m.verified) add("⚠ signature BAD")
                        if (m.mine && m.id != null) add(if (m.delivered) "✓ delivered" else "· sent")
                        if (m.mine && m.fragments > 1) add("⇡ ${m.fragments} fragments")
                    }
                    Text(bits.joinToString("  ·  "), style = MaterialTheme.typography.bodySmall)
                }
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            OutlinedButton(onClick = { pickFile.launch("*/*") }) { Text("📎") }
            Spacer(Modifier.width(8.dp))
            OutlinedTextField(value = text, onValueChange = { text = it }, modifier = Modifier.weight(1f))
            Spacer(Modifier.width(8.dp))
            Button(onClick = { NodeController.send(peer, text); text = "" }) { Text("Send") }
        }
    }
}

@Composable
private fun FeedScreen() {
    val posts by NodeController.posts.collectAsState()
    val topics by NodeController.topics.collectAsState()
    var follow by remember { mutableStateOf("") }
    var compose by remember { mutableStateOf("") }
    var activeTopic by remember { mutableStateOf<String?>(null) }
    val shown = remember(posts, activeTopic) {
        if (activeTopic == null) posts else posts.filter { it.topic == activeTopic }
    }

    Column(Modifier.padding(16.dp).fillMaxSize()) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(
                value = follow, onValueChange = { follow = it },
                label = { Text("follow a topic, e.g. spore/news") }, modifier = Modifier.weight(1f)
            )
            Spacer(Modifier.width(8.dp))
            OutlinedButton(onClick = { NodeController.follow(follow); follow = "" }) { Text("Follow") }
        }
        Spacer(Modifier.height(8.dp))
        // Topic chips read across, not down — a one-line filter strip.
        LazyRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            items(listOf<String?>(null) + topics) { t ->
                Text(
                    if (t == null) "all" else "#$t",
                    Modifier.padding(vertical = 8.dp).clickable { activeTopic = t },
                    style = if (activeTopic == t) MaterialTheme.typography.titleSmall else MaterialTheme.typography.bodySmall
                )
            }
        }
        if (shown.isEmpty()) {
            Text("nothing sprouting here yet 🌱", Modifier.fillMaxWidth().padding(24.dp), textAlign = TextAlign.Center)
        }
        LazyColumn(Modifier.weight(1f)) {
            items(shown.asReversed()) { p ->
                Card(Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
                    Column(Modifier.padding(12.dp)) {
                        Text("#${p.topic} · ${Petnames.label(p.author)}${if (!p.verified) " ⚠" else ""}", style = MaterialTheme.typography.bodySmall)
                        Text(p.text)
                    }
                }
            }
        }
        val target = activeTopic ?: topics.firstOrNull()
        Row(verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(
                value = compose, onValueChange = { compose = it },
                label = { Text(if (target != null) "post to #$target" else "follow a topic first") },
                modifier = Modifier.weight(1f)
            )
            Spacer(Modifier.width(8.dp))
            Button(
                onClick = { if (target != null) { NodeController.post(target, compose); compose = "" } },
                enabled = target != null
            ) { Text("Post") }
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

    LazyColumn(Modifier.padding(16.dp).fillMaxSize(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        val pending = found
        if (pending == null) {
            item {
                Card(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(12.dp)) {
                        Text("Your invite", style = MaterialTheme.typography.titleSmall)
                        Text(
                            "Let a friend scan this. It shares your address, your name, and any " +
                                "relay or swarm bridges you're on.",
                            style = MaterialTheme.typography.bodySmall
                        )
                        Spacer(Modifier.height(8.dp))
                        if (invite.isNotBlank()) QrImage(invite)
                        Text(invite, style = MaterialTheme.typography.bodySmall)
                        Row(Modifier.padding(top = 8.dp)) {
                            OutlinedButton(onClick = {
                                val cm = ctx.getSystemService(android.content.ClipboardManager::class.java)
                                cm?.setPrimaryClip(android.content.ClipData.newPlainText("SPORE invite", invite))
                            }) { Text("Copy") }
                            Spacer(Modifier.width(8.dp))
                            OutlinedButton(onClick = {
                                val i = Intent(Intent.ACTION_SEND).apply {
                                    type = "text/plain"; putExtra(Intent.EXTRA_TEXT, invite)
                                }
                                ctx.startActivity(Intent.createChooser(i, "Share your SPORE invite"))
                            }) { Text("Share") }
                        }
                    }
                }
            }
            item {
                Card(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(12.dp)) {
                        Text("Add a friend", style = MaterialTheme.typography.titleSmall)
                        if (scanning) {
                            QrScanner(onResult = { text ->
                                val inv = NodeController.parseInvite(text)
                                if (inv != null) accept(inv) else note = "that QR isn't a SPORE invite"
                            })
                            OutlinedButton(onClick = { scanning = false }) { Text("Stop scanning") }
                        } else {
                            OutlinedButton(onClick = {
                                if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.CAMERA)
                                    == PackageManager.PERMISSION_GRANTED
                                ) scanning = true else camPerm.launch(Manifest.permission.CAMERA)
                            }) { Text("Scan their QR") }
                        }
                        Spacer(Modifier.height(8.dp))
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            OutlinedTextField(
                                value = pasted, onValueChange = { pasted = it },
                                label = { Text("…or paste an invite") }, modifier = Modifier.weight(1f)
                            )
                            Spacer(Modifier.width(8.dp))
                            OutlinedButton(onClick = {
                                val inv = NodeController.parseInvite(pasted)
                                if (inv != null) { accept(inv); pasted = "" }
                                else note = "that doesn't look like a valid invite"
                            }) { Text("Add") }
                        }
                        if (note.isNotBlank()) {
                            Text(note, style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
            }
        } else {
            item {
                Card(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(12.dp)) {
                        Text("Add ${pending.suggestedName.ifBlank { "this node" }}?", style = MaterialTheme.typography.titleSmall)
                        Text("address ${pending.addr}", style = MaterialTheme.typography.bodySmall)
                        Text(
                            "The name in an invite is only what they claim — the petname you set " +
                                "here is the one you'll trust.",
                            style = MaterialTheme.typography.bodySmall
                        )
                        Spacer(Modifier.height(8.dp))
                        OutlinedTextField(
                            value = petname, onValueChange = { petname = it },
                            label = { Text("petname") }, modifier = Modifier.fillMaxWidth()
                        )
                    }
                }
            }
            if (pending.bridges.isNotEmpty()) {
                item {
                    Card(Modifier.fillMaxWidth()) {
                        Column(Modifier.padding(12.dp)) {
                            Text("Also join their bridges?", style = MaterialTheme.typography.titleSmall)
                            Text(
                                "Only tick these if you trust the sender: joining connects your " +
                                    "node to servers they chose.",
                                style = MaterialTheme.typography.bodySmall
                            )
                            pending.bridges.forEach { b ->
                                Row(
                                    Modifier.fillMaxWidth().padding(top = 6.dp).clickable {
                                        chosen = if (b in chosen) chosen - b else chosen + b
                                    },
                                    verticalAlignment = Alignment.CenterVertically
                                ) {
                                    Text(if (b in chosen) "☑" else "☐")
                                    Spacer(Modifier.width(8.dp))
                                    Text(b, style = MaterialTheme.typography.bodySmall)
                                }
                            }
                        }
                    }
                }
            }
            item {
                Row {
                    Button(onClick = {
                        NodeController.acceptInvite(pending, petname)
                        if (chosen.isNotEmpty()) NodeController.applyInviteBridges(ctx, chosen.toList())
                        note = "added ${petname.ifBlank { pending.suggestedName }}"
                        found = null
                    }) { Text("Add contact") }
                    Spacer(Modifier.width(8.dp))
                    OutlinedButton(onClick = { found = null }) { Text("Cancel") }
                }
            }
        }
    }
}

@Composable
private fun AdvancedScreen(addr: String) {
    val ctx = LocalContext.current
    val topics by NodeController.topics.collectAsState()
    var showSeed by remember { mutableStateOf(false) }
    Column(Modifier.padding(16.dp).fillMaxSize(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        val myName by NodeController.myName.collectAsState()
        var editName by remember(myName) { mutableStateOf(myName) }
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(12.dp)) {
                Text("Your name", style = MaterialTheme.typography.titleSmall)
                Text(
                    "Announced to nodes in range, and offered to them as the default petname " +
                        "for you. A display hint, not an identity — your address is that.",
                    style = MaterialTheme.typography.bodySmall
                )
                Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(
                        value = editName, onValueChange = { editName = it },
                        label = { Text("name") }, modifier = Modifier.weight(1f)
                    )
                    Spacer(Modifier.width(8.dp))
                    OutlinedButton(onClick = { NodeController.setMyName(editName) }) { Text("Save") }
                }
            }
        }
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(12.dp)) {
                Text("Identity", style = MaterialTheme.typography.titleSmall)
                Text("address: $addr", style = MaterialTheme.typography.bodySmall)
                Text(
                    "followed topics: ${if (topics.isEmpty()) "—" else topics.joinToString(", ")}",
                    style = MaterialTheme.typography.bodySmall
                )
            }
        }
        val nearby by NodeController.peers.collectAsState()
        val stored by NodeController.storeCount.collectAsState()
        val resumed by NodeController.resumed.collectAsState()
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(12.dp)) {
                Text("Node", style = MaterialTheme.typography.titleSmall)
                Text("peers heard: ${nearby.size}", style = MaterialTheme.typography.bodySmall)
                Text(
                    "can encrypt to: ${nearby.count { it.hasKey }} of them",
                    style = MaterialTheme.typography.bodySmall
                )
                Text("envelopes relayed/stored: $stored", style = MaterialTheme.typography.bodySmall)
                if (resumed > 0) {
                    Text(
                        "picked up $resumed from storage on start — transfers resume " +
                            "rather than restart 🌱",
                        style = MaterialTheme.typography.bodySmall
                    )
                }
                Text(
                    "Direct messages are sealed to a peer's key once you've heard their " +
                        "announce; broadcasts and topic posts are signed but public by nature.",
                    style = MaterialTheme.typography.bodySmall
                )
            }
        }
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(12.dp)) {
                Text("Seed (your whole identity)", style = MaterialTheme.typography.titleSmall)
                Text(
                    "Anyone holding these 32 bytes IS this node. Only reveal to move " +
                        "your identity to another device.",
                    style = MaterialTheme.typography.bodySmall
                )
                Spacer(Modifier.height(8.dp))
                if (!showSeed) {
                    OutlinedButton(onClick = { showSeed = true }) { Text("Reveal seed") }
                } else {
                    val seedHex = remember {
                        ctx.getSharedPreferences("spore", android.content.Context.MODE_PRIVATE)
                            .getString("seed", null)
                            ?.let { android.util.Base64.decode(it, android.util.Base64.NO_WRAP) }
                            ?.joinToString("") { b -> "%02x".format(b) } ?: "unavailable"
                    }
                    Text(seedHex, style = MaterialTheme.typography.bodySmall)
                    OutlinedButton(onClick = { showSeed = false }) { Text("Hide") }
                }
            }
        }
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(12.dp)) {
                Text("About", style = MaterialTheme.typography.titleSmall)
                Text(
                    "SPORE — store-and-forward planetary opportunistic relay envelope. " +
                        "This phone is a full node: it signs, relays, and delivers across " +
                        "every enabled bridge. Public domain. 🍄",
                    style = MaterialTheme.typography.bodySmall
                )
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

    LazyColumn(Modifier.padding(16.dp).fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        item {
            Text(
                "Bridges relay your signed envelopes across every medium at once. 🍄",
                style = MaterialTheme.typography.bodySmall
            )
        }
        items(bridges) { b ->
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(12.dp)) {
                    Text("${b.kind} · ${b.status}", style = MaterialTheme.typography.titleSmall)
                    Text(b.detail, style = MaterialTheme.typography.bodySmall)
                }
            }
        }
        item {
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = tcp, onValueChange = { tcp = it },
                    label = { Text("TCP host:port (blank = listen)") }, modifier = Modifier.weight(1f)
                )
                Spacer(Modifier.width(8.dp))
                OutlinedButton(onClick = { NodeController.addTcp(tcp.trim()); tcp = "" }) { Text("Add") }
            }
        }
        item {
            OutlinedButton(onClick = {
                withPerms(listOf(Manifest.permission.RECORD_AUDIO)) { NodeController.enableAudio() }
            }, modifier = Modifier.fillMaxWidth()) { Text("Enable audio modem (mic + speaker)") }
        }
        item {
            OutlinedButton(onClick = {
                withPerms(blePerms) { showMeshPick = !showMeshPick }
            }, modifier = Modifier.fillMaxWidth()) { Text("Add Meshtastic radio (paired BLE)") }
        }
        if (showMeshPick) {
            items(bonded()) { d ->
                Card(Modifier.fillMaxWidth().clickable {
                    NodeController.enableMeshtasticBle(ctx, d); showMeshPick = false
                }) { Text("📻 ${try { d.name } catch (_: SecurityException) { null } ?: d.address}", Modifier.padding(12.dp)) }
            }
        }
        item {
            OutlinedButton(onClick = {
                withPerms(blePerms) { showRnodePick = !showRnodePick }
            }, modifier = Modifier.fillMaxWidth()) { Text("Add Reticulum RNode (paired BLE)") }
        }
        if (showRnodePick) {
            item {
                Row {
                    OutlinedTextField(freq, { freq = it }, label = { Text("MHz") }, modifier = Modifier.weight(1f))
                    Spacer(Modifier.width(4.dp))
                    OutlinedTextField(bw, { bw = it }, label = { Text("kHz") }, modifier = Modifier.weight(1f))
                    Spacer(Modifier.width(4.dp))
                    OutlinedTextField(sf, { sf = it }, label = { Text("SF") }, modifier = Modifier.weight(0.7f))
                    Spacer(Modifier.width(4.dp))
                    OutlinedTextField(cr, { cr = it }, label = { Text("CR") }, modifier = Modifier.weight(0.7f))
                    Spacer(Modifier.width(4.dp))
                    OutlinedTextField(tx, { tx = it }, label = { Text("dBm") }, modifier = Modifier.weight(0.7f))
                }
            }
            items(bonded()) { d ->
                Card(Modifier.fillMaxWidth().clickable {
                    val f = ((freq.toDoubleOrNull() ?: 867.2) * 1e6).toLong()
                    val b = ((bw.toDoubleOrNull() ?: 125.0) * 1e3).toLong()
                    NodeController.enableRNodeBle(
                        ctx, d, f, b, sf.toIntOrNull() ?: 8, cr.toIntOrNull() ?: 5, tx.toIntOrNull() ?: 0
                    )
                    showRnodePick = false
                }) { Text("📡 ${try { d.name } catch (_: SecurityException) { null } ?: d.address}", Modifier.padding(12.dp)) }
            }
        }
        item {
            OutlinedButton(onClick = {
                withPerms(wifiP2pPerms) { NodeController.enableWifiDirect(ctx) }
            }, modifier = Modifier.fillMaxWidth()) { Text("Enable Wi-Fi Direct group") }
        }
        item {
            var ws by remember { mutableStateOf("") }
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = ws, onValueChange = { ws = it },
                    label = { Text("WebSocket relay wss://…") }, modifier = Modifier.weight(1f)
                )
                Spacer(Modifier.width(8.dp))
                OutlinedButton(onClick = { NodeController.addWebSocket(ctx, ws); ws = "" }) { Text("Add") }
            }
        }
        item {
            var nostr by remember { mutableStateOf("") }
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = nostr, onValueChange = { nostr = it },
                    label = { Text("Nostr relay wss://… (rx-only)") }, modifier = Modifier.weight(1f)
                )
                Spacer(Modifier.width(8.dp))
                OutlinedButton(onClick = { NodeController.addNostr(ctx, nostr); nostr = "" }) { Text("Add") }
            }
        }
        item {
            var swarm by remember { mutableStateOf("spore/public") }
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = swarm, onValueChange = { swarm = it },
                    label = { Text("WebTorrent swarm name") }, modifier = Modifier.weight(1f)
                )
                Spacer(Modifier.width(8.dp))
                OutlinedButton(onClick = { NodeController.addWebTorrent(ctx, swarm) }) { Text("Join") }
            }
        }
    }
}
