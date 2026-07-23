package org.spore.node

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
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
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat

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
private data object BridgesScreen : Screen

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun App() {
    MaterialTheme {
        // Ask for notification permission on 33+ so the foreground node is visible.
        val ctx = androidx.compose.ui.platform.LocalContext.current
        val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {}
        androidx.compose.runtime.LaunchedEffect(Unit) {
            if (Build.VERSION.SDK_INT >= 33 &&
                ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
            ) ask.launch(Manifest.permission.POST_NOTIFICATIONS)
        }

        var screen by remember { mutableStateOf<Screen>(Chats) }
        val addr by NodeController.address.collectAsState()

        Scaffold(
            topBar = {
                TopAppBar(title = {
                    when (val s = screen) {
                        is Chat -> Text("🍄 " + Petnames.label(s.peer))
                        BridgesScreen -> Text("🍄 Bridges")
                        else -> Text("🍄 SPORE")
                    }
                }, navigationIcon = {
                    if (screen is Chat) {
                        IconButton(onClick = { screen = Chats }) { Text("←") }
                    }
                })
            },
            bottomBar = {
                if (screen !is Chat) {
                    NavigationBar {
                        NavigationBarItem(
                            selected = screen == Chats,
                            onClick = { screen = Chats },
                            icon = {}, label = { Text("Chats") }
                        )
                        NavigationBarItem(
                            selected = screen == BridgesScreen,
                            onClick = { screen = BridgesScreen },
                            icon = {}, label = { Text("Bridges") }
                        )
                    }
                }
            }
        ) { pad ->
            Column(Modifier.padding(pad).fillMaxSize()) {
                when (val s = screen) {
                    Chats -> ChatsList(addr) { screen = Chat(it) }
                    is Chat -> ChatDetail(s.peer)
                    BridgesScreen -> BridgesList()
                }
            }
        }
    }
}

@Composable
private fun ChatsList(addr: String, open: (String) -> Unit) {
    val messages by NodeController.messages.collectAsState()
    val names by Petnames.map.collectAsState()
    var newPeer by remember { mutableStateOf("") }

    val peers = remember(messages, names) {
        (messages.map { it.peer } + Petnames.PUBLIC + names.keys).distinct()
    }
    Column(Modifier.padding(16.dp).fillMaxSize()) {
        Text("your addr $addr", style = MaterialTheme.typography.bodySmall)
        Spacer(Modifier.height(8.dp))
        Row {
            OutlinedTextField(
                value = newPeer, onValueChange = { newPeer = it },
                label = { Text("open chat with address (16 hex)") }, modifier = Modifier.weight(1f)
            )
            Spacer(Modifier.width(8.dp))
            OutlinedButton(onClick = {
                val h = newPeer.trim().lowercase()
                if (h.length == 16) { open(h); newPeer = "" }
            }) { Text("Open") }
        }
        Spacer(Modifier.height(8.dp))
        if (peers.isEmpty()) {
            Text("no spores nearby yet 🍄", Modifier.fillMaxWidth().padding(32.dp), textAlign = TextAlign.Center)
        }
        LazyColumn(Modifier.weight(1f)) {
            items(peers) { peer ->
                val last = messages.lastOrNull { it.peer == peer }?.text ?: "—"
                Card(Modifier.fillMaxWidth().padding(vertical = 4.dp).clickable { open(peer) }) {
                    Column(Modifier.padding(12.dp)) {
                        Text(Petnames.label(peer), style = MaterialTheme.typography.titleMedium)
                        Text(last, style = MaterialTheme.typography.bodySmall, maxLines = 1)
                    }
                }
            }
        }
    }
}

@Composable
private fun ChatDetail(peer: String) {
    val messages by NodeController.messages.collectAsState()
    val names by Petnames.map.collectAsState()
    var text by remember { mutableStateOf("") }
    var editingName by remember(peer) { mutableStateOf(names[peer] ?: "") }
    val thread = remember(messages, peer) { messages.filter { it.peer == peer } }

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
                val sig = if (!m.mine && !m.verified) "  ⚠ sig BAD" else ""
                Text("${if (m.mine) "▶" else "◀"} $who: ${m.text}$sig", Modifier.padding(vertical = 2.dp))
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(value = text, onValueChange = { text = it }, modifier = Modifier.weight(1f))
            Spacer(Modifier.width(8.dp))
            Button(onClick = { NodeController.send(peer, text); text = "" }) { Text("Send") }
        }
    }
}

@Composable
private fun BridgesList() {
    val ctx = androidx.compose.ui.platform.LocalContext.current
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
