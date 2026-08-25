#!/usr/bin/env python3
"""Check that a flashed board is actually working, and watch it.

Reading a log and squinting at it is not a test. This connects over serial,
waits for the firmware to say what it is, and turns that into pass/fail —
including the two things only a running board can answer: whether the crypto
works on this silicon, and whether the heap holds steady over time.

    ./diagnose.py                 # run the checks, print a verdict, exit 0/1
    ./diagnose.py --monitor       # checks, then stay attached as a terminal
    ./diagnose.py --monitor-only  # skip the checks, just be a terminal
    ./diagnose.py --port /dev/ttyUSB0 --seconds 90

Boards whose console is their own USB port disappear from /dev when they
reset, so every read here tolerates the port vanishing and coming back rather
than treating it as failure — which it looks like, and isn't.
"""
import argparse
import re
import sys
import time

try:
    import serial
except ImportError:
    sys.exit("pyserial is missing:  pip3 install --user --break-system-packages pyserial")

# The repeating line the firmware prints every sixth tick, e.g.
#   up 35s · addr=b1aea40a34b8f146 · sig=ok · heap=226396 · due=0
SUMMARY = re.compile(
    r"up (?P<up>\d+)s .*?addr=(?P<addr>[0-9a-f]+) .*?sig=(?P<sig>\w+) "
    r".*?heap=(?P<heap>\d+) .*?due=(?P<due>\d+)"
)
# Boot-only lines, caught if we happen to attach in time or after a reset.
IDENTITY = re.compile(r"identity: addr=(?P<addr>[0-9a-f]+)")
SIGNED = re.compile(r"signed envelope: (?P<len>\d+) bytes, id=(?P<id>[0-9a-f]+), verify=(?P<ok>\w+)")
PROBE = re.compile(r"probe=Some\((?P<probe>\d+)\) \(wire is (?P<wire>\d+) bytes\), decoded ok=(?P<ok>\w+)")
PANIC = re.compile(r"(?i)panic|guru meditation|rst:0x|abort\(\)")


class Checks:
    """Every finding is either observed or absent — never assumed."""

    def __init__(self):
        self.summaries = []
        self.identity = None
        self.signed = None
        self.probe = None
        self.panics = []
        self.lines = 0

    def feed(self, line):
        self.lines += 1
        if m := SUMMARY.search(line):
            self.summaries.append({k: m.group(k) for k in ("up", "addr", "sig", "heap", "due")})
        if m := IDENTITY.search(line):
            self.identity = m.group("addr")
        if m := SIGNED.search(line):
            self.signed = m.groupdict()
        if m := PROBE.search(line):
            self.probe = m.groupdict()
        if PANIC.search(line):
            self.panics.append(line.strip())

    def verdict(self):
        """-> [(ok|None, name, detail)]. None means 'not observed', not 'failed'."""
        out = []
        out.append((self.lines > 0, "board is talking", f"{self.lines} lines read"))

        addr = self.identity or (self.summaries[-1]["addr"] if self.summaries else None)
        out.append((
            bool(addr) and len(addr) == 16 and all(c in "0123456789abcdef" for c in addr),
            "identity", f"addr={addr}" if addr else "no address seen",
        ))

        # The crypto question: did a signature this board made verify on this
        # board? Accepted from either the boot line or the repeating summary.
        sig = None
        if self.signed:
            sig = self.signed["ok"] == "true"
        elif self.summaries:
            sig = self.summaries[-1]["sig"] == "ok"
        out.append((sig, "signature verifies", "sig=ok" if sig else "not observed" if sig is None else "SIGNATURE FAILED"))

        if self.probe:
            agree = self.probe["probe"] == self.probe["wire"] and self.probe["ok"] == "true"
            out.append((agree, "probe agrees with decode", f"{self.probe['probe']}=={self.probe['wire']} bytes"))
        else:
            out.append((None, "probe agrees with decode", "boot-only line, not seen"))

        # Ticks must advance, or the scheduling nutrient is not being supplied
        # and the node only maintains itself when traffic happens to arrive.
        if len(self.summaries) >= 2:
            ups = [int(s["up"]) for s in self.summaries]
            out.append((ups[-1] > ups[0], "scheduler is ticking",
                        f"uptime {ups[0]}s -> {ups[-1]}s over {len(ups)} summaries"))
            # A leak shows up here and nowhere else — static section sizes
            # cannot see it, and a single reading cannot either.
            heaps = [int(s["heap"]) for s in self.summaries]
            # Signed as the change in free heap, so negative means memory lost.
            delta = heaps[-1] - heaps[0]
            out.append((delta > -1024, "heap is stable",
                        f"{heaps[0]} -> {heaps[-1]} bytes ({delta:+d})"))
        else:
            need = "need 2 summaries, saw %d" % len(self.summaries)
            out.append((None, "scheduler is ticking", need))
            out.append((None, "heap is stable", need))

        out.append((not self.panics, "no panics or resets",
                    self.panics[0] if self.panics else "clean"))
        return out


def open_port(port, baud, wait_s=30, quiet=False):
    """Wait for the port to exist and open. It vanishes across a board reset."""
    deadline = time.time() + wait_s
    announced = False
    while time.time() < deadline:
        try:
            return serial.Serial(port, baud, timeout=0.2)
        except (serial.SerialException, OSError):
            if not announced and not quiet:
                print(f"waiting for {port} ...", end="", flush=True)
                announced = True
            elif not quiet:
                print(".", end="", flush=True)
            time.sleep(0.5)
    if announced and not quiet:
        print()
    return None


def pulse_reset(ser):
    """Reset the board so the boot lines can be caught.

    Some checks — the probe/decode agreement especially — are only printed
    once at startup, and on a board whose console is its own USB port those
    are gone before a host can attach. Toggling DTR/RTS puts the chip through
    reset with us already listening. Best-effort: not every board wires it,
    and the caller falls back to the repeating summary.
    """
    try:
        ser.dtr = False
        ser.rts = True
        time.sleep(0.15)
        ser.rts = False
        time.sleep(0.05)
        return True
    except (serial.SerialException, OSError):
        return False


def stream(port, baud, seconds, on_line, echo, reset=False):
    """Read lines for `seconds`, surviving re-enumeration mid-run."""
    deadline = time.time() + seconds
    buf = b""
    # Do not spend longer looking for the port than the caller asked to observe
    # for, but always allow a few seconds — the node reappears a moment after a
    # reset and an instant failure would just be a race.
    ser = open_port(port, baud, wait_s=max(5, min(30, seconds)))
    if ser is None:
        return False
    print(f"connected to {port} at {baud}")
    if reset:
        print("pulsing reset to catch the boot lines ...")
        pulse_reset(ser)
    print()
    while time.time() < deadline:
        try:
            chunk = ser.read(256)
        except (serial.SerialException, OSError):
            # The board reset; the node will come back under the same name.
            try:
                ser.close()
            except Exception:
                pass
            ser = open_port(port, baud, wait_s=15, quiet=True)
            if ser is None:
                return True
            continue
        if not chunk:
            continue
        buf += chunk
        while b"\n" in buf:
            raw, buf = buf.split(b"\n", 1)
            line = raw.decode("utf-8", "replace").rstrip("\r")
            if echo:
                print(line, flush=True)
            on_line(line)
    return True


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", default="/dev/ttyACM0")
    ap.add_argument("--baud", type=int, default=115200)
    ap.add_argument("--seconds", type=int, default=45,
                    help="how long to observe; needs >2 summary lines, which arrive every ~30s")
    ap.add_argument("--monitor", action="store_true", help="stay attached after the checks")
    ap.add_argument("--monitor-only", action="store_true", help="no checks, just watch")
    ap.add_argument("--quiet", action="store_true", help="verdict only, do not echo the log")
    ap.add_argument("--reset", action="store_true",
                    help="pulse DTR/RTS first, to catch the boot-only lines")
    args = ap.parse_args()

    if args.monitor_only:
        print("Ctrl-C to stop.\n")
        try:
            stream(args.port, args.baud, 10**9, lambda _l: None, echo=True)
        except KeyboardInterrupt:
            print("\n--- detached ---")
        return 0

    checks = Checks()
    print(f"Observing {args.port} for {args.seconds}s. The summary repeats every ~30s,")
    print("so this needs to run at least that long to judge ticking and heap.\n")
    try:
        if not stream(args.port, args.baud, args.seconds, checks.feed,
                      echo=not args.quiet, reset=args.reset):
            print(f"\nnothing at {args.port}.", file=sys.stderr)
            print("Is it flashed and running? Tap RST to leave download mode.", file=sys.stderr)
            return 1
    except KeyboardInterrupt:
        print("\n(interrupted — judging on what was seen)")

    print("\n── Diagnosis " + "─" * 49)
    failed = False
    for ok, name, detail in checks.verdict():
        mark = "PASS" if ok else ("....." if ok is None else "FAIL")
        print(f"  [{mark:^5}] {name:<26} {detail}")
        if ok is False:
            failed = True
    print("─" * 62)

    if any(ok is None for ok, _, _ in checks.verdict()):
        print("\n'.....' means not observed in this window — run longer with --seconds,")
        print("or re-flash to catch the boot-only lines. It is not a failure.")
    print("FAILED" if failed else "\nAll observed checks passed.")

    if args.monitor:
        print("\n--- monitoring, Ctrl-C to stop ---")
        try:
            stream(args.port, args.baud, 10**9, lambda _l: None, echo=True)
        except KeyboardInterrupt:
            print("\n--- detached ---")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
