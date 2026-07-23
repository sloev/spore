package org.spore.node

import android.annotation.SuppressLint
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Base64
import android.webkit.JavascriptInterface
import android.webkit.WebView
import android.webkit.WebViewClient
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * Headless WebView carrying the web-origin bridges — WebSocket, Nostr, and
 * WebTorrent (peer-to-peer WebRTC under the hood) — by running the repo's real
 * `web/transports/*.mjs` (copied into assets at build time) and piping raw
 * envelopes to the native node over one JNI iface. Loaded with an https base URL
 * so `crypto.subtle` / WebRTC get a secure context.
 */
@SuppressLint("SetJavaScriptEnabled")
class WebBridgeHost(private val ctx: Context, private val ptr: Long) {
    private val iface: Int = SporeNative.nativeRegisterIface(ptr)
    private var webView: WebView? = null
    private var pump: Job? = null
    private val main = Handler(Looper.getMainLooper())
    private var ready = false
    private val queued = ArrayList<String>() // JS calls issued before page load

    var onEvent: ((String) -> Unit)? = null

    inner class Host {
        @JavascriptInterface
        fun onFrame(b64: String) {
            val bytes = try { Base64.decode(b64, Base64.NO_WRAP) } catch (_: Exception) { return }
            SporeNative.nativePushRx(ptr, iface, bytes)
        }

        @JavascriptInterface
        fun onEvent(msg: String) {
            this@WebBridgeHost.onEvent?.invoke(msg)
        }
    }

    fun start() {
        main.post {
            val wv = WebView(ctx)
            webView = wv
            wv.settings.javaScriptEnabled = true
            wv.settings.mediaPlaybackRequiresUserGesture = false
            wv.addJavascriptInterface(Host(), "SporeHost")
            wv.webViewClient = object : WebViewClient() {
                override fun onPageFinished(view: WebView, url: String) {
                    inject(view)
                }
            }
            // An https base URL makes this a secure context (crypto.subtle, WebRTC).
            wv.loadDataWithBaseURL("https://spore.invalid/", "<html><body></body></html>", "text/html", "utf-8", null)
        }
        // Outbound pump: node forwards → every JS transport.
        pump = CoroutineScope(Dispatchers.IO).launch {
            while (isActive) {
                val out = SporeNative.nativePollForward(ptr, iface)
                if (out != null) {
                    val b64 = Base64.encodeToString(out, Base64.NO_WRAP)
                    eval("sporeForward('$b64')")
                } else delay(60)
            }
        }
    }

    private fun inject(view: WebView) {
        // The transports import { Transport } from '../spore.mjs'; strip the
        // imports/exports (same rule as web/build-standalone.mjs) and provide the
        // tiny base class + glue ourselves. The node stays native.
        val sources = listOf("websocket.mjs", "nostr.mjs", "webtorrent.mjs").joinToString("\n\n") { name ->
            ctx.assets.open("webtransports/$name").bufferedReader().readText()
                .lines()
                .filterNot { it.trim().matches(Regex("^import\\s.+from\\s.+;?$")) }
                .joinToString("\n") { it.replace(Regex("^\\s*export\\s+"), "") }
        }
        val glue = """
            class Transport { send(_b) {} receive(_b) {} }
            $sources
            const __ts = [];
            const __b64 = (u8) => { let s=''; for (const b of u8) s += String.fromCharCode(b); return btoa(s); };
            const __un = (s) => { const b = atob(s); const u = new Uint8Array(b.length); for (let i=0;i<b.length;i++) u[i]=b.charCodeAt(i); return u; };
            function __attach(t, label) {
              t.receive = (bytes) => SporeHost.onFrame(__b64(bytes));
              __ts.push(t); SporeHost.onEvent(label + ' up');
            }
            function sporeForward(b64) {
              const bytes = __un(b64);
              for (const t of __ts) { try { t.send(bytes); } catch (e) {} }
            }
            function sporeAddWebSocket(url) { try { __attach(new WebSocketTransport(url), 'WebSocket ' + url); } catch (e) { SporeHost.onEvent('WebSocket error: ' + e.message); } }
            function sporeAddNostr(url) { try { __attach(new NostrTransport(url, null), 'Nostr ' + url + ' (rx-only)'); } catch (e) { SporeHost.onEvent('Nostr error: ' + e.message); } }
            function sporeAddWebTorrent(name) {
              WebTorrentTransport.join(name).then((t) => {
                t.onpeer = (n) => SporeHost.onEvent('WebTorrent ' + name + ': ' + n + ' peer(s)');
                __attach(t, 'WebTorrent ' + name);
              }).catch((e) => SporeHost.onEvent('WebTorrent error: ' + e.message));
            }
        """.trimIndent()
        view.evaluateJavascript(glue, null)
        ready = true
        queued.forEach { view.evaluateJavascript(it, null) }
        queued.clear()
    }

    private fun eval(js: String) {
        main.post {
            val wv = webView ?: return@post
            if (ready) wv.evaluateJavascript(js, null) else queued.add(js)
        }
    }

    fun addWebSocket(url: String) = eval("sporeAddWebSocket(${url.jsQuote()})")
    fun addNostr(url: String) = eval("sporeAddNostr(${url.jsQuote()})")
    fun addWebTorrent(name: String) = eval("sporeAddWebTorrent(${name.jsQuote()})")

    fun stop() {
        pump?.cancel()
        main.post { webView?.destroy(); webView = null }
    }

    private fun String.jsQuote() = "'" + replace("\\", "\\\\").replace("'", "\\'") + "'"
}
