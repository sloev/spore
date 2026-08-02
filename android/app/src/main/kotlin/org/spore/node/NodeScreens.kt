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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

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
    var showRingExport by remember { mutableStateOf(false) }
    var confirmRingExport by remember { mutableStateOf(false) }
    val ringHealth by NodeController.ringHealth.collectAsState()
    val offlineWindowSecs by NodeController.offlineWindowSecs.collectAsState()
    var customOfflineDays by remember { mutableStateOf("") }
    // A raise above the default goes through a confirm first (PR0 Part B) —
    // this holds the pending value while that dialog is up, null otherwise.
    var pendingOfflineWindowSecs by remember { mutableStateOf<Int?>(null) }
    val confirm = rememberConfirm()
    val myName by NodeController.myName.collectAsState()
    var editName by remember(myName) { mutableStateOf(myName) }
    val avatarPath by NodeController.myAvatarPath.collectAsState()
    val nearby by NodeController.peers.collectAsState()
    val stored by NodeController.storeCount.collectAsState()
    val resumed by NodeController.resumed.collectAsState()
    val ctx = LocalContext.current
    val confirmAvatar = rememberConfirm()

    // Pick an image, downscale it off the main thread, hand the small bytes to the
    // controller. Capped hard (max edge 256 px, JPEG) so an avatar is cheap to
    // store now and cheap to flood in PR4b.
    val pickAvatar = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri != null) {
            val bytes = ctx.contentResolver.openInputStream(uri)?.use { it.readBytes() }
            if (bytes != null) {
                val small = downscaleAvatar(bytes)
                if (small != null && NodeController.setAvatar(small)) confirmAvatar("Photo updated")
                else confirmAvatar("Could not read that image")
            }
        }
    }

    // Below the core's own default: takes effect immediately. Above it: gated
    // behind the raise-above-default confirm, since a longer window trades
    // convenience for how much a stolen device could still read (PR0 Part B).
    fun requestOfflineWindow(days: Int) {
        val secs = days * 86_400
        if (secs > NodeController.DEFAULT_OFFLINE_WINDOW_SECS) {
            pendingOfflineWindowSecs = secs
        } else {
            val ok = NodeController.setOfflineWindowSecs(secs)
            confirm(if (ok) "Offline window set to ${days}d" else "Node not started yet — not saved")
        }
    }

    LazyColumn(
        Modifier.padding(12.dp).fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        item {
            Crate(Modifier.fillMaxWidth()) {
                Column {
                    // "Name others see" — the field is public-facing, so frame it as
                    // what a peer will call you by, not as a private setting.
                    DisplayHeading("Name others see", size = 15)
                    Caption("Announced to nodes in range as a suggested petname and shown with your posts. A hint, not an identity.")
                    VGap()
                    // Live preview: the avatar + name exactly as a peer's Nearby row
                    // renders them (ChatsList uses the same letter fallback).
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        ProfilePic(avatarPath, editName.ifBlank { addr }, 44)
                        HGap()
                        Column(Modifier.weight(1f)) {
                            Text(
                                editName.trim().take(32).ifBlank { "…${addr.takeLast(6)}" },
                                color = Palette.Amber, fontWeight = FontWeight.Bold, maxLines = 1,
                            )
                            Caption("this is how you appear to others")
                        }
                        CrateButton(if (avatarPath == null) "Add photo" else "Change", { pickAvatar.launch("image/*") })
                    }
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
                    DisplayHeading("Prekey ring", size = 15)
                    Caption("The keys others seal mail to, so it can be read only until they rotate. A copy of the ring undoes that for whatever it still holds.")
                    VGap()
                    val h = ringHealth
                    if (h == null) {
                        Caption("unavailable — node not started yet")
                    } else {
                        Caption("held: ${h.count} · oldest: ${h.oldestAgeSecs?.let { formatDuration(it) } ?: "unknown"}")
                        Caption(
                            if (h.nextMintInSecs <= 0) "next rotation: due"
                            else "next rotation: in ${formatDuration(h.nextMintInSecs)}"
                        )
                    }
                    VGap()
                    if (!showRingExport) {
                        CrateButton("Export ring", { confirmRingExport = true })
                    } else {
                        val ringHex = remember { NodeController.prekeyRingHex() ?: "unavailable" }
                        Text(ringHex, color = Palette.Amber)
                        VGap(4.dp)
                        CrateButton("Hide", { showRingExport = false })
                    }
                }
            }
        }

        item {
            Crate(Modifier.fillMaxWidth()) {
                Column {
                    DisplayHeading("Offline window", size = 15)
                    Caption("How long a device can be offline and still read mail sealed before it left — and how far back a lost or stolen device could read. Longer trades security for convenience.")
                    VGap()
                    Caption("current: ${formatDuration(offlineWindowSecs)}")
                    VGap()
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        listOf(7, 14, 30).forEach { d ->
                            CrateButton("${d}d", { requestOfflineWindow(d) }, enabled = offlineWindowSecs != d * 86_400)
                        }
                    }
                    VGap()
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        ToughbookField(
                            customOfflineDays, { customOfflineDays = it }, Modifier.weight(1f),
                            placeholder = "custom days",
                        )
                        HGap()
                        CrateButton(
                            "Set",
                            {
                                val d = customOfflineDays.toIntOrNull()
                                if (d != null && d > 0) requestOfflineWindow(d)
                                else confirm("Enter a whole number of days")
                            },
                        )
                    }
                }
            }
        }

        item {
            Crate(Modifier.fillMaxWidth()) {
                Column {
                    DisplayHeading("About", size = 15)
                    Caption("SPORE — store-and-forward planetary opportunistic relay envelope. This phone is a full node: it signs, relays, and delivers across every enabled bridge. Public domain. 🍄")
                    VGap(6.dp)
                    val offlineDays = offlineWindowSecs / 86_400
                    Caption("Forward secrecy: the keys others seal mail to (prekeys) rotate on a $offlineDays-day window, and a conversation's keys ratchet forward per message — a key kept to decrypt an out-of-order message is dropped after $offlineDays days too. Losing today's device doesn't expose messages older than that window. Your seed is stored in Android's encrypted preferences and excluded from cloud/adb backup.")
                }
            }
        }
    }

    if (confirmRingExport) {
        ConfirmDialog(
            title = "Export the prekey ring?",
            body = "This defeats the 7-day forward-secrecy window for whatever a copy still " +
                "holds — anyone with it can read old mail sealed to those keys for as long as " +
                "the copy exists, even after the live ring has rotated past them.",
            confirmLabel = "Show ring",
            onConfirm = { confirmRingExport = false; showRingExport = true },
            onDismiss = { confirmRingExport = false },
        )
    }

    pendingOfflineWindowSecs?.let { secs ->
        val days = secs / 86_400
        ConfirmDialog(
            title = "Raise the offline window to ${days}d?",
            body = "A longer window means a lost or stolen device — or a copy of the prekey " +
                "ring — can still read that much more mail after the fact. Only raise this if " +
                "you need to be offline that long and accept the trade.",
            confirmLabel = "Set ${days}d",
            onConfirm = {
                val ok = NodeController.setOfflineWindowSecs(secs)
                confirm(if (ok) "Offline window set to ${days}d" else "Node not started yet — not saved")
                pendingOfflineWindowSecs = null
            },
            onDismiss = { pendingOfflineWindowSecs = null },
        )
    }
}

/** Render a duration in the largest whole unit that keeps it short (B5 ring health). */
private fun formatDuration(seconds: Int): String = when {
    seconds < 60 -> "${seconds}s"
    seconds < 3600 -> "${seconds / 60}m"
    seconds < 86400 -> "${seconds / 3600}h"
    else -> "${seconds / 86400}d"
}

@Composable
internal fun BridgesList() {
    val ctx = LocalContext.current
    val bridges by NodeController.bridges.collectAsState()
    var tcp by remember { mutableStateOf("") }
    var pendingAction by remember { mutableStateOf<(() -> Unit)?>(null) }
    var pendingLabel by remember { mutableStateOf("") }
    // A denial used to just dead-end silently — no row, no message, nothing to
    // do about it. This names what needs the permission and offers the one real
    // recovery: Android's own per-app settings screen.
    var deniedFor by remember { mutableStateOf<String?>(null) }
    val askPerms = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { granted ->
        if (granted.values.all { it }) pendingAction?.invoke() else deniedFor = pendingLabel
        pendingAction = null
    }

    fun withPerms(label: String, perms: List<String>, action: () -> Unit) {
        val missing = perms.filter {
            ContextCompat.checkSelfPermission(ctx, it) != PackageManager.PERMISSION_GRANTED
        }
        if (missing.isEmpty()) action()
        else { pendingAction = action; pendingLabel = label; askPerms.launch(missing.toTypedArray()) }
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
        if (bridges.isEmpty()) {
            item {
                Caption(
                    "no bridges yet 🍄\nadd one below — SPORE only reaches as far as its bridges do",
                    Modifier.fillMaxWidth().padding(vertical = 12.dp),
                )
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
                withPerms("Audio modem", listOf(Manifest.permission.RECORD_AUDIO)) { NodeController.enableAudio() }
            }, Modifier.padding(vertical = 4.dp))
        }
        item {
            CrateButton("Meshtastic (paired BLE)", {
                withPerms("Meshtastic BLE", blePerms) { showMeshPick = !showMeshPick }
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
                withPerms("Reticulum RNode", blePerms) { showRnodePick = !showRnodePick }
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
                withPerms("Wi-Fi Direct", wifiP2pPerms) { NodeController.enableWifiDirect(ctx) }
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

    deniedFor?.let { label ->
        ConfirmDialog(
            title = "$label needs a permission",
            body = "Android reported it as denied, so this can't start. Open the app's " +
                "system settings to grant it, then try again.",
            confirmLabel = "Open settings",
            onConfirm = {
                deniedFor = null
                ctx.startActivity(
                    Intent(
                        android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                        android.net.Uri.fromParts("package", ctx.packageName, null),
                    )
                )
            },
            onDismiss = { deniedFor = null },
        )
    }
}

/** The four states a bridge row's LED actually distinguishes (B6). */
private enum class BridgeStatus { Up, Connecting, Down, Error }

/**
 * Map a bridge's free-text status to [BridgeStatus] — an exact match over the
 * small, known vocabulary every source emits (`NodeController`'s own literals,
 * `BleBridge`, `WifiDirectBridge`, `WebBridgeHost`'s events), not the blind
 * substring `in` checks this replaces.
 *
 * Those substring checks were silently wrong in exactly the cases a status LED
 * exists to get right: `"disconnected"` contains `"connect"`, so it read as
 * *connecting*; `"unsupported"` contains `"up"`, so it read as *up*. An unknown
 * string (an error message we haven't enumerated) falls to `Connecting` rather
 * than guessing green or red.
 */
private fun classifyBridgeStatus(status: String): BridgeStatus {
    val s = status.lowercase()
    return when {
        s.contains("error") -> BridgeStatus.Error
        s == "unsupported" || s == "disconnected" || s == "stopped" -> BridgeStatus.Down
        s == "connecting" || s == "discovering" || s == "reconnecting" ||
            s == "starting" || s == "group requested" || s == "joining existing" -> BridgeStatus.Connecting
        s == "on" || s == "open" || s == "up" || s == "group up" ||
            s.endsWith(" up") || s.contains("peer(") -> BridgeStatus.Up
        else -> BridgeStatus.Connecting
    }
}

/**
 * One bridge: an LED dot, the kind, its status line, and — for a bridge this app
 * can actually stop — Pause/Resume and Remove controls.
 *
 * `canStop` is the honest gate (§ VISUALDESIGN / audit "no fake UI"): every
 * bridge kind gets a real Remove now (PR2 carried forward gave the core-owned
 * UDP/TCP bridges a stop hook too, not just the Kotlin-driven ones), but the
 * flag stays rather than assuming every future kind can be — a bridge we
 * genuinely cannot stop should show a plain caption, not a dead button.
 * `canToggle` is the same honesty gate for a separate Pause/Resume: only
 * offered where a Resume can restart with the exact configuration the row
 * already shows, not a button that quietly comes back to something else (see
 * `NodeController.toggleBridge`'s doc for which bridges qualify and why).
 */
@Composable
private fun BridgeRow(b: BridgeState) {
    val (dot, label) = when (classifyBridgeStatus(b.status)) {
        BridgeStatus.Up -> Palette.Phosphor to b.status
        BridgeStatus.Connecting -> Palette.Amber to b.status
        BridgeStatus.Down -> Palette.Moss to b.status
        // Never signal failure by colour alone (§ VISUALDESIGN): pair pink with an icon.
        BridgeStatus.Error -> Palette.Pink to "⚠ ${b.status}"
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
            if (b.canToggle) {
                HGap(6.dp)
                CrateButton(
                    if (b.enabled) "Pause" else "Resume",
                    { NodeController.toggleBridge(b) },
                    contentDescription = if (b.enabled) "Pause ${b.kind}" else "Resume ${b.kind}",
                )
            }
            if (b.canStop) {
                HGap(6.dp)
                CrateButton("Remove", { NodeController.stopBridge(b) })
            }
        }
    }
}

/**
 * A profile picture: the avatar image at [path] if present, else the same moss
 * letter tile [ChatsList]'s Nearby rows use, so a node with no photo still reads
 * as itself. Square tile with the machined-metal border, per VISUALDESIGN §3.
 */
@Composable
internal fun ProfilePic(path: String?, name: String, size: Int = 34) {
    if (path != null) {
        val bmp by produceState<android.graphics.Bitmap?>(null, path) {
            value = withContext(Dispatchers.IO) {
                runCatching { android.graphics.BitmapFactory.decodeFile(path) }.getOrNull()
            }
        }
        val shown = bmp
        if (shown != null) {
            Image(
                shown.asImageBitmap(),
                contentDescription = "profile picture",
                modifier = Modifier.size(size.dp).border(2.dp, Palette.Edge, CrateShape),
                contentScale = ContentScale.Crop,
            )
            return
        }
    }
    Box(
        Modifier.size(size.dp).background(Palette.Moss, CrateShape).border(2.dp, Palette.Edge, CrateShape),
        contentAlignment = Alignment.Center,
    ) {
        // Amber on moss is 4.27:1 — large text only (§1), which a bold initial is.
        Text(name.firstOrNull()?.uppercase() ?: "?", color = Palette.Amber, fontWeight = FontWeight.Bold)
    }
}

/**
 * Decode picked image bytes and re-encode a small JPEG (max edge 256 px). Returns
 * null if the bytes aren't a decodable image. `inSampleSize` keeps the initial
 * decode cheap — a phone photo is tens of megapixels — then an exact scale hits
 * the 256 px cap, so an avatar stays a few KB whether it's cached (PR4a) or floods
 * the mesh (PR4b).
 */
private fun downscaleAvatar(bytes: ByteArray): ByteArray? = runCatching {
    val max = 256
    val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
    android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    var sample = 1
    while (bounds.outWidth / sample > max * 2 || bounds.outHeight / sample > max * 2) sample *= 2
    val decoded = android.graphics.BitmapFactory.decodeByteArray(
        bytes, 0, bytes.size,
        android.graphics.BitmapFactory.Options().apply { inSampleSize = sample },
    ) ?: return@runCatching null
    val scale = (max.toFloat() / maxOf(decoded.width, decoded.height)).coerceAtMost(1f)
    val out = if (scale < 1f) {
        android.graphics.Bitmap.createScaledBitmap(
            decoded, (decoded.width * scale).toInt().coerceAtLeast(1),
            (decoded.height * scale).toInt().coerceAtLeast(1), true,
        )
    } else {
        decoded
    }
    val baos = java.io.ByteArrayOutputStream()
    out.compress(android.graphics.Bitmap.CompressFormat.JPEG, 80, baos)
    baos.toByteArray()
}.getOrNull()
