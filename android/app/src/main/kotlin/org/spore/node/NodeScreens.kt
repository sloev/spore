package org.spore.node

/*
 * The screens about this node rather than about messages: meeting someone, the
 * identity and storage panel, and the bridge list.
 */

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
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
import androidx.core.content.ContextCompat

/**
 * Meet someone: show them your QR, or scan/paste theirs. An invite carries your
 * address, the name you announce, and the bridges you're reachable on — so they
 * can join the same mesh, not merely learn a number.
 */
@Composable
internal fun ConnectScreen() {
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
internal fun AdvancedScreen(addr: String) {
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
internal fun BridgesList() {
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
