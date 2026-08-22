package org.spore.node

/*
 * The app shell: the Activity, the screen graph, the crate-lid app bar, the bottom
 * nav, and the two status strips above the content.
 *
 * Screens live beside this file — ChatScreens.kt, FeedScreens.kt, NodeScreens.kt —
 * and the reusable chrome they are built from is in Chrome.kt. This was one
 * 1200-line file; the split is by screen group so a change to the feed does not
 * mean scrolling past the bridge list.
 */

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.selection.selectable
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat

import kotlinx.coroutines.launch

/**
 * The app's one snackbar host, reachable from any screen.
 *
 * Both Save buttons live several composables below the [Scaffold] that owns the
 * host, and neither had any way to say "done". The buttons were never broken —
 * they persisted on the first click all along — they just looked broken, which
 * is the same bug as far as anyone using the app is concerned.
 */
internal val LocalSnackbar = staticCompositionLocalOf<SnackbarHostState> {
    error("no SnackbarHostState in scope — App() provides it")
}

/** Confirm an action that otherwise leaves no trace on screen. */
@Composable
internal fun rememberConfirm(): (String) -> Unit {
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

internal sealed interface Screen
internal data object Chats : Screen
internal data class Chat(val peer: String) : Screen
internal data object Feed : Screen
internal data class Compose(val topic: String) : Screen
internal data object BridgesScreen : Screen
internal data object Advanced : Screen
internal data object Connect : Screen

/** Wall-clock HH:mm for a message stamp. */
internal fun timeOf(ts: Long): String =
    java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault()).format(java.util.Date(ts))

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

        // System Back mirrors the ← arrow's hierarchy instead of leaving the app
        // from a nested screen: a thread falls to the chats list, a draft post to
        // the feed, everything else to Chats. Only Chats itself lets Back through
        // to background the app.
        BackHandler(enabled = screen != Chats) {
            screen = when (screen) {
                is Chat -> Chats
                is Compose -> Feed
                else -> Chats
            }
        }

        Scaffold(
            snackbarHost = { SnackbarHost(snackbar) },
            topBar = { TopBar(screen) { screen = it } },
            bottomBar = {
                if (screen.showsNav()) BottomNav(screen) { screen = it }
            },
            containerColor = MaterialTheme.colorScheme.background,
        ) { pad ->
            CompositionLocalProvider(LocalSnackbar provides snackbar) {
                Column(Modifier.padding(pad).fillMaxSize()) {
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
            IconTap("←", "Back", color = Palette.Ink) { go(if (screen is Compose) Feed else Chats) }
        }
        Column(Modifier.weight(1f)) {
            DisplayHeading(screen.title())
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
            StickerBadge("${peers.size} peers", ink = Palette.Ink)
            HGap(6.dp)
            IconTap("👋", "Connect") { go(Connect) }
            IconTap("⚙", "Advanced settings") { go(Advanced) }
        }
    }
}

/**
 * A minimal icon/emoji-only tap target: a real 48dp-minimum touch area with an
 * accessible name — TalkBack otherwise reads only the raw glyph (B7).
 */
@Composable
private fun IconTap(glyph: String, description: String, color: Color = Color.Unspecified, onClick: () -> Unit) {
    Box(
        Modifier
            .sizeIn(minWidth = 48.dp, minHeight = 48.dp)
            .clickable(onClick = onClick)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        Text(glyph, color = color, modifier = Modifier.clearAndSetSemantics {})
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
                    Modifier
                        .weight(1f)
                        .heightIn(min = 48.dp) // was ~33dp — under the floor (B7)
                        // Announces the selected tab to TalkBack, which colour
                        // alone (the pink dot) does not (B7).
                        .selectable(selected = on, onClick = { go(target) }, role = Role.Tab),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Box(
                        Modifier
                            .size(14.dp)
                            .then(
                                if (on) Modifier.background(Palette.Yellow)
                                else Modifier.border(1.dp, Palette.Muted)
                            )
                    )
                    VGap(3.dp)
                    Caption(label.uppercase(), color = if (on) Palette.Yellow else Palette.Muted)
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
        val rest = active.size - 3
        if (rest > 0) Caption("+$rest more", color = Palette.Muted)
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
