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
    val bridges by NodeController.bridges.collectAsState()
    var tcp by remember { mutableStateOf("") }
    Column(Modifier.padding(16.dp).fillMaxSize()) {
        Text("Bridges relay your signed envelopes across every medium at once.", style = MaterialTheme.typography.bodySmall)
        Spacer(Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(
                value = tcp, onValueChange = { tcp = it },
                label = { Text("add TCP (host:port, blank = listen)") }, modifier = Modifier.weight(1f)
            )
            Spacer(Modifier.width(8.dp))
            OutlinedButton(onClick = { NodeController.addTcp(tcp.trim()); tcp = "" }) { Text("Add") }
        }
        Spacer(Modifier.height(8.dp))
        LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            items(bridges) { b ->
                Card(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(12.dp)) {
                        Text("${b.kind} · ${b.status}", style = MaterialTheme.typography.titleSmall)
                        Text(b.detail, style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
        }
    }
}
