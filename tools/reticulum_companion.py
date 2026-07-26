#!/usr/bin/env python3
"""RNS companion for SPORE's `reticulum` bridge.

The SPORE daemon (`spore reticulum`) speaks **KISS-framed envelopes** on its
stdin/stdout; this companion moves them over a real **Reticulum** network as the
data of packets sent to a shared **PLAIN** destination — a broadcast bus that
every SPORE-over-RNS node listens on. Reticulum supplies the transport, path-
finding, and every interface it is configured with (LoRa, TCP, I2P, packet
radio); SPORE's envelope carries its own Ed25519 signature and optional
encryption, so nothing security-critical lives here.

The bus is the PLAIN destination ``spore.mesh`` (app name + aspect). Because a
PLAIN destination's hash is derived only from its name and aspects, every node
computes the same destination and hears every packet on it — exactly SPORE's
flood model. The daemon clamps its MTU so each envelope fits one RNS packet;
larger objects fountain-fragment above the bridge like on any medium.

Requires the Reticulum library:  ``pip install rns``

Wiring (bidirectional — two fifos connect the daemon and this companion)::

    mkfifo /tmp/spore_up /tmp/spore_down
    python3 tools/reticulum_companion.py < /tmp/spore_up > /tmp/spore_down &
    spore reticulum  < /tmp/spore_down > /tmp/spore_up

An optional argument is a Reticulum config path (defaults to RNS's own default).
"""
import queue
import sys
import threading

try:
    import RNS
except ImportError:
    sys.stderr.write(
        "reticulum_companion: the Reticulum library is required — `pip install rns`\n"
    )
    sys.exit(1)

APP_NAME = "spore"
ASPECT = "mesh"  # the shared bus; all SPORE-over-RNS nodes use the same

# --- KISS framing (matches src/kiss.rs and the daemon's stream bridges) --------
FEND, FESC, TFEND, TFESC = 0xC0, 0xDB, 0xDC, 0xDD


def kiss_encode(data: bytes) -> bytes:
    out = bytearray([FEND, 0x00])  # FEND + command byte (data, port 0)
    for b in data:
        if b == FEND:
            out += bytes([FESC, TFEND])
        elif b == FESC:
            out += bytes([FESC, TFESC])
        else:
            out.append(b)
    out.append(FEND)
    return bytes(out)


class KissDecoder:
    """Streaming KISS de-framer — yields whole frames across chunked reads."""

    def __init__(self):
        self.cur = bytearray()
        self.in_frame = False
        self.got_cmd = False
        self.esc = False

    def push(self, chunk: bytes):
        frames = []
        for b in chunk:
            if b == FEND:
                if self.in_frame and self.cur:
                    frames.append(bytes(self.cur))
                self.in_frame, self.got_cmd, self.esc = True, False, False
                self.cur = bytearray()
            elif not self.in_frame:
                continue
            elif not self.got_cmd:
                self.got_cmd = True  # drop the KISS command byte
            elif self.esc:
                self.cur.append(FEND if b == TFEND else FESC if b == TFESC else b)
                self.esc = False
            elif b == FESC:
                self.esc = True
            else:
                self.cur.append(b)
        return frames


def _parse_listen(argv):
    """Return (kind, port) for a `--listen tcp:PORT` / `udp:PORT` arg, else None.
    Everything else (an RNS config path) is left for RNS.Reticulum()."""
    for i, a in enumerate(argv):
        if a == "--listen" and i + 1 < len(argv):
            spec = argv[i + 1]
        elif a.startswith("--listen="):
            spec = a.split("=", 1)[1]
        else:
            continue
        kind, _, port = spec.partition(":")
        if kind in ("tcp", "udp") and port.isdigit():
            return kind, int(port)
        raise SystemExit(f"bad --listen {spec!r}; want tcp:PORT or udp:PORT")
    return None


def _serve_tcp(port, on_frame_in, frames_out):
    """One SPORE daemon at a time over TCP. `frames_out` is a queue of raw
    envelopes to KISS-frame outward; `on_frame_in(frame)` gets each inbound one."""
    import socket
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("0.0.0.0", port))
    srv.listen(1)
    RNS.log(f"SPORE companion listening on tcp:{port}", RNS.LOG_INFO)
    while True:
        conn, _ = srv.accept()
        RNS.log("SPORE daemon connected", RNS.LOG_INFO)
        alive = [True]

        def pump_out(c=conn, a=alive):
            while a[0]:
                try:
                    frame = frames_out.get(timeout=0.2)
                except queue.Empty:
                    continue
                try:
                    c.sendall(kiss_encode(frame))
                except OSError:
                    a[0] = False
                    return
        threading.Thread(target=pump_out, daemon=True).start()

        dec = KissDecoder()
        try:
            while True:
                chunk = conn.recv(4096)
                if not chunk:
                    break
                for frame in dec.push(chunk):
                    on_frame_in(frame)
        finally:
            alive[0] = False
            conn.close()
            RNS.log("SPORE daemon disconnected; awaiting the next", RNS.LOG_INFO)


def _serve_udp(port, on_frame_in, frames_out, peer_holder):
    import socket
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind(("0.0.0.0", port))
    RNS.log(f"SPORE companion listening on udp:{port}", RNS.LOG_INFO)

    def pump_out():
        while True:
            frame = frames_out.get()
            peer = peer_holder.get("addr")
            if peer:
                try:
                    sock.sendto(kiss_encode(frame), peer)
                except OSError:
                    pass
    threading.Thread(target=pump_out, daemon=True).start()

    dec = KissDecoder()
    while True:
        data, addr = sock.recvfrom(4096)
        peer_holder["addr"] = addr  # reply to whoever last spoke
        for frame in dec.push(data):
            on_frame_in(frame)


def main():
    listen = _parse_listen(sys.argv[1:])
    # The RNS config path is the first non-flag argument, if any.
    config_path = None
    skip = False
    for a in sys.argv[1:]:
        if skip:
            skip = False
            continue
        if a == "--listen":
            skip = True
        elif a.startswith("--listen=") or a.startswith("--"):
            continue
        else:
            config_path = a
            break
    RNS.Reticulum(config_path)

    # IN destination: receive packets on the shared bus.
    rx_dest = RNS.Destination(
        None, RNS.Destination.IN, RNS.Destination.PLAIN, APP_NAME, ASPECT
    )
    # OUT destination: same name/aspect ⇒ same hash ⇒ the same bus.
    tx_dest = RNS.Destination(
        None, RNS.Destination.OUT, RNS.Destination.PLAIN, APP_NAME, ASPECT
    )

    RNS.log("SPORE ⇄ RNS bus up on PLAIN destination "
            + RNS.prettyhexrep(rx_dest.hash), RNS.LOG_INFO)

    def send_to_rns(frame):
        RNS.Packet(tx_dest, frame).send()

    if listen:
        # Network transport: an inbound RNS packet is queued out to the daemon;
        # a KISS frame from the daemon goes onto the RNS bus.
        frames_out = queue.Queue()
        rx_dest.set_packet_callback(lambda data, _p: frames_out.put(data))
        kind, port = listen
        if kind == "tcp":
            _serve_tcp(port, send_to_rns, frames_out)
        else:
            _serve_udp(port, send_to_rns, frames_out, {})
        return

    # Default transport: stdio pipes (the original behaviour).
    out = sys.stdout.buffer
    out_lock = threading.Lock()

    def on_packet(data, _packet):
        with out_lock:
            out.write(kiss_encode(data))
            out.flush()

    rx_dest.set_packet_callback(on_packet)
    dec = KissDecoder()
    stdin = sys.stdin.buffer
    while True:
        chunk = stdin.read(4096)
        if not chunk:
            break  # daemon closed the pipe
        for frame in dec.push(chunk):
            send_to_rns(frame)


if __name__ == "__main__":
    main()
