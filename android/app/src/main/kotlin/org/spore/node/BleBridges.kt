package org.spore.node

import android.annotation.SuppressLint
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCallback
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothGattDescriptor
import android.bluetooth.BluetoothProfile
import android.content.Context
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import java.util.UUID

// The standard client-characteristic-configuration descriptor (enables notify).
private val CCCD = UUID.fromString("00002902-0000-1000-8000-00805f9b34fb")

/**
 * Base for a Kotlin BLE bridge pumped through the JNI iface poll API. Subclasses
 * translate between SPORE envelopes and the device's frames. All calls are gated
 * on BLUETOOTH_CONNECT by the UI before start.
 */
@SuppressLint("MissingPermission")
abstract class BleBridge(
    protected val ptr: Long,
    protected val iface: Int,
    private val ctx: Context,
    private val device: BluetoothDevice,
) {
    protected var gatt: BluetoothGatt? = null
    private var txJob: Job? = null
    var onState: ((String) -> Unit)? = null

    /** The hub interface id this bridge drives, so the controller can unregister it. */
    val ifaceId: Int get() = iface

    /** Called once services are discovered; set up characteristics + notifies. */
    abstract fun onReady(g: BluetoothGatt)

    /** Transmit one SPORE envelope to the device. */
    abstract fun sendEnvelope(g: BluetoothGatt, env: ByteArray)

    /** A notification arrived on `c`. */
    abstract fun onNotify(g: BluetoothGatt, c: BluetoothGattCharacteristic, value: ByteArray)

    fun start() {
        gatt = device.connectGatt(ctx, false, object : BluetoothGattCallback() {
            override fun onConnectionStateChange(g: BluetoothGatt, status: Int, newState: Int) {
                if (newState == BluetoothProfile.STATE_CONNECTED) {
                    onState?.invoke("discovering")
                    g.requestMtu(247)
                } else if (newState == BluetoothProfile.STATE_DISCONNECTED) {
                    onState?.invoke("disconnected")
                    txJob?.cancel()
                }
            }

            override fun onMtuChanged(g: BluetoothGatt, mtu: Int, status: Int) {
                g.discoverServices()
            }

            override fun onServicesDiscovered(g: BluetoothGatt, status: Int) {
                if (status == BluetoothGatt.GATT_SUCCESS) {
                    onReady(g)
                    onState?.invoke("open")
                    // TX pump: node forwards → device.
                    txJob = CoroutineScope(Dispatchers.IO).launch {
                        while (isActive) {
                            val out = SporeNative.nativePollForward(ptr, iface)
                            if (out != null) sendEnvelope(g, out) else delay(60)
                        }
                    }
                } else onState?.invoke("error")
            }

            @Deprecated("pre-33 callback; fine for our minSdk")
            override fun onCharacteristicChanged(g: BluetoothGatt, c: BluetoothGattCharacteristic) {
                @Suppress("DEPRECATION")
                onNotify(g, c, c.value ?: return)
            }
        })
    }

    fun stop() {
        txJob?.cancel()
        try { gatt?.disconnect(); gatt?.close() } catch (_: Exception) {}
    }

    protected fun enableNotify(g: BluetoothGatt, c: BluetoothGattCharacteristic) {
        g.setCharacteristicNotification(c, true)
        val d = c.getDescriptor(CCCD) ?: return
        @Suppress("DEPRECATION")
        d.value = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
        @Suppress("DEPRECATION")
        g.writeDescriptor(d)
    }

    @Suppress("DEPRECATION")
    protected fun writeChunked(g: BluetoothGatt, c: BluetoothGattCharacteristic, data: ByteArray, chunk: Int = 20) {
        var i = 0
        while (i < data.size) {
            val end = minOf(i + chunk, data.size)
            c.value = data.copyOfRange(i, end)
            c.writeType = BluetoothGattCharacteristic.WRITE_TYPE_NO_RESPONSE
            g.writeCharacteristic(c)
            i = end
            Thread.sleep(15) // pace writes; no per-write callback plumbing needed
        }
    }
}

/**
 * Meshtastic BLE bridge: SPORE envelopes ride MeshPackets on portnum 256 via the
 * ToRadio/FromRadio characteristics — the phone twin of `bridge::meshtastic`.
 */
@SuppressLint("MissingPermission")
class MeshtasticBleBridge(ptr: Long, iface: Int, ctx: Context, device: BluetoothDevice, private val myNode: Int) :
    BleBridge(ptr, iface, ctx, device) {

    companion object {
        val SERVICE: UUID = UUID.fromString("6ba1b218-15a8-461f-9fa8-5dcae273eafd")
        val TORADIO: UUID = UUID.fromString("f75c76d2-129e-4dad-a1dd-7866124401e7")
        val FROMRADIO: UUID = UUID.fromString("2c55e69e-4993-11ed-b878-0242ac120002")
        val FROMNUM: UUID = UUID.fromString("ed9da18c-a800-4f66-a670-aa7547e34453")
    }

    private var toRadio: BluetoothGattCharacteristic? = null
    private var fromRadio: BluetoothGattCharacteristic? = null
    private var pktId = (Math.random() * Int.MAX_VALUE).toInt()

    override fun onReady(g: BluetoothGatt) {
        val svc = g.getService(SERVICE) ?: return
        toRadio = svc.getCharacteristic(TORADIO)
        fromRadio = svc.getCharacteristic(FROMRADIO)
        svc.getCharacteristic(FROMNUM)?.let { enableNotify(g, it) }
        drain(g)
    }

    override fun sendEnvelope(g: BluetoothGatt, env: ByteArray) {
        val c = toRadio ?: return
        pktId += 1
        val pkt = SporeNative.nativeMeshtasticWrap(env, myNode, pktId) ?: return
        // ToRadio { 1: packet } — tag 0x0A, varint length, then the MeshPacket.
        val out = ArrayList<Byte>(pkt.size + 4)
        out.add(0x0A)
        var n = pkt.size
        while (true) {
            val b = n and 0x7f
            n = n ushr 7
            out.add((if (n != 0) b or 0x80 else b).toByte())
            if (n == 0) break
        }
        pkt.forEach { p -> out.add(p) }
        writeChunked(g, c, out.toByteArray(), 200)
    }

    override fun onNotify(g: BluetoothGatt, c: BluetoothGattCharacteristic, value: ByteArray) {
        if (c.uuid == FROMNUM) drain(g)
    }

    /** FromNum says packets are waiting: read FromRadio until empty. */
    @Suppress("DEPRECATION")
    private fun drain(g: BluetoothGatt) {
        CoroutineScope(Dispatchers.IO).launch {
            val c = fromRadio ?: return@launch
            repeat(32) {
                if (!g.readCharacteristic(c)) return@launch
                delay(80) // allow the read callback to populate c.value
                val v = c.value ?: return@launch
                if (v.isEmpty()) return@launch
                // FromRadio { 2: packet } → strip the tag+len, unwrap the MeshPacket.
                val pkt = stripField2(v) ?: return@launch
                val env = SporeNative.nativeMeshtasticUnwrap(pkt)
                if (env != null) SporeNative.nativePushRx(ptr, iface, env)
            }
        }
    }

    /** Extract field 2 (len-delimited) from a tiny FromRadio protobuf. */
    private fun stripField2(b: ByteArray): ByteArray? {
        var i = 0
        while (i < b.size) {
            val tag = b[i].toInt() and 0xff; i++
            val field = tag ushr 3; val wire = tag and 7
            if (wire == 2) {
                var len = 0; var shift = 0
                while (i < b.size) {
                    val x = b[i].toInt() and 0xff; i++
                    len = len or ((x and 0x7f) shl shift)
                    if (x < 0x80) break
                    shift += 7
                }
                if (i + len > b.size) return null
                if (field == 2) return b.copyOfRange(i, i + len)
                i += len
            } else if (wire == 0) {
                while (i < b.size && (b[i].toInt() and 0x80) != 0) i++
                i++
            } else return null
        }
        return null
    }
}

/**
 * Reticulum / RNode BLE bridge: RNode host/KISS mode over the Nordic UART
 * Service. Configure the radio, then each envelope is a KISS DATA frame — the
 * phone twin of `web/transports/reticulum.mjs`.
 */
@SuppressLint("MissingPermission")
class RNodeBleBridge(
    ptr: Long, iface: Int, ctx: Context, device: BluetoothDevice,
    private val freqHz: Long, private val bwHz: Long, private val sf: Int, private val cr: Int, private val txDbm: Int,
) : BleBridge(ptr, iface, ctx, device) {

    companion object {
        val NUS: UUID = UUID.fromString("6e400001-b5a3-f393-e0a9-e50e24dcca9e")
        val NUS_RX: UUID = UUID.fromString("6e400002-b5a3-f393-e0a9-e50e24dcca9e")
        val NUS_TX: UUID = UUID.fromString("6e400003-b5a3-f393-e0a9-e50e24dcca9e")
        private const val FEND = 0xC0; private const val FESC = 0xDB
        private const val TFEND = 0xDC; private const val TFESC = 0xDD
        private const val CMD_DATA = 0x00
    }

    private var rx: BluetoothGattCharacteristic? = null
    private val cur = ArrayList<Byte>()
    private var inFrame = false; private var gotCmd = false; private var esc = false; private var cmd = -1

    private fun kissCmd(c: Int, data: ByteArray): ByteArray {
        val out = ArrayList<Byte>(data.size + 4)
        out.add(FEND.toByte()); out.add(c.toByte())
        for (b in data) {
            when (b.toInt() and 0xff) {
                FEND -> { out.add(FESC.toByte()); out.add(TFEND.toByte()) }
                FESC -> { out.add(FESC.toByte()); out.add(TFESC.toByte()) }
                else -> out.add(b)
            }
        }
        out.add(FEND.toByte())
        return out.toByteArray()
    }

    private fun be32(n: Long) = byteArrayOf(
        ((n ushr 24) and 0xff).toByte(), ((n ushr 16) and 0xff).toByte(),
        ((n ushr 8) and 0xff).toByte(), (n and 0xff).toByte()
    )

    override fun onReady(g: BluetoothGatt) {
        val svc = g.getService(NUS) ?: return
        rx = svc.getCharacteristic(NUS_RX)
        svc.getCharacteristic(NUS_TX)?.let { enableNotify(g, it) }
        val c = rx ?: return
        // Bring the radio up (freq/bw/sf/cr/txpower, then RADIO_STATE=1).
        writeChunked(g, c, kissCmd(0x01, be32(freqHz)))
        writeChunked(g, c, kissCmd(0x02, be32(bwHz)))
        writeChunked(g, c, kissCmd(0x03, byteArrayOf(txDbm.toByte())))
        writeChunked(g, c, kissCmd(0x04, byteArrayOf(sf.toByte())))
        writeChunked(g, c, kissCmd(0x05, byteArrayOf(cr.toByte())))
        writeChunked(g, c, kissCmd(0x06, byteArrayOf(1)))
    }

    override fun sendEnvelope(g: BluetoothGatt, env: ByteArray) {
        val c = rx ?: return
        writeChunked(g, c, kissCmd(CMD_DATA, env))
    }

    override fun onNotify(g: BluetoothGatt, c: BluetoothGattCharacteristic, value: ByteArray) {
        // Streaming KISS de-framer that keeps the command byte; DATA frames only.
        for (byte in value) {
            val b = byte.toInt() and 0xff
            if (b == FEND) {
                if (inFrame && gotCmd && cmd == CMD_DATA && cur.isNotEmpty()) {
                    SporeNative.nativePushRx(ptr, iface, cur.toByteArray())
                }
                inFrame = true; gotCmd = false; esc = false; cur.clear()
            } else if (!inFrame) {
                // skip
            } else if (!gotCmd) {
                cmd = b; gotCmd = true
            } else if (esc) {
                cur.add((if (b == TFEND) FEND else if (b == TFESC) FESC else b).toByte()); esc = false
            } else if (b == FESC) {
                esc = true
            } else {
                cur.add(b.toByte())
            }
        }
    }
}
