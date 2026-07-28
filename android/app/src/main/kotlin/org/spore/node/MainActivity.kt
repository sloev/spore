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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.size
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.delay

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
