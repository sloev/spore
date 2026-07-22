#!/usr/bin/env python3
"""Generate the C header and the Python / Go / JS wrappers from spec.json.

    python3 bindings/generate.py

The spec mirrors the C ABI in src/ffi.rs. Each function's arguments use a tiny
type vocabulary:

    bytes            a byte slice        -> C: (const uint8_t*, size_t)
    in8/in16/in32    fixed input buffer  -> C: const uint8_t*
    out8/out16/out32 fixed output buffer -> C: uint8_t*   (caller-allocated)
    u32              a u32               -> C: uint32_t

Return is one of: void | bytes (an owned SporeBytes) | bool (uint8_t).
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
SPEC = json.load(open(os.path.join(HERE, "spec.json")))
FNS = SPEC["functions"]

OUT_SIZE = {"out8": 8, "out16": 16, "out32": 32}
IN_SIZE = {"in8": 8, "in16": 16, "in32": 32}


def camel(name):
    return "".join(p.capitalize() for p in name.split("_"))


def inputs(fn):
    """User-facing input args (everything but out*)."""
    return [a for a in fn["args"] if a["t"] not in OUT_SIZE]


def outs(fn):
    return [a for a in fn["args"] if a["t"] in OUT_SIZE]


# --------------------------------------------------------------------------- C
def gen_c():
    lines = [
        "/* Auto-generated from bindings/spec.json — do not edit. */",
        "#ifndef SPORE_H",
        "#define SPORE_H",
        "#include <stdint.h>",
        "#include <stddef.h>",
        "",
        "typedef struct { uint8_t *data; size_t len; } SporeBytes;",
        "void spore_bytes_free(SporeBytes b);",
        "",
    ]
    for fn in FNS:
        ret = {"void": "void", "bytes": "SporeBytes", "bool": "uint8_t"}[fn["ret"]]
        params = []
        for a in fn["args"]:
            t, n = a["t"], a["n"]
            if t == "bytes":
                params += [f"const uint8_t *{n}", f"size_t {n}_len"]
            elif t in IN_SIZE:
                params.append(f"const uint8_t *{n}")
            elif t in OUT_SIZE:
                params.append(f"uint8_t *{n}")
            elif t == "u32":
                params.append(f"uint32_t {n}")
        lines.append(f"{ret} spore_{fn['name']}({', '.join(params)});")
    lines += ["", "#endif", ""]
    return "\n".join(lines)


# ---------------------------------------------------------------------- Python
PY_PRELUDE = '''\
"""Auto-generated SPORE bindings (ctypes) — do not edit; run bindings/generate.py.

Loads libspore from $SPORE_LIB or ../../target/release/. Every function takes and
returns `bytes`; functions that can fail return `None`.
"""
import ctypes
import os

def _load():
    names = ("libspore.so", "libspore.dylib", "spore.dll")
    here = os.path.dirname(os.path.abspath(__file__))
    tried = []
    if os.environ.get("SPORE_LIB"):
        tried.append(os.environ["SPORE_LIB"])
    for n in names:
        tried.append(os.path.join(here, "..", "..", "target", "release", n))
        tried.append(n)
    last = None
    for path in tried:
        try:
            return ctypes.CDLL(path)
        except OSError as e:
            last = e
    raise last

_l = _load()

class SporeBytes(ctypes.Structure):
    _fields_ = [("data", ctypes.POINTER(ctypes.c_ubyte)), ("len", ctypes.c_size_t)]

_l.spore_bytes_free.argtypes = [SporeBytes]
_l.spore_bytes_free.restype = None

def _take(b):
    if not b.data:
        return None
    out = ctypes.string_at(b.data, b.len)
    _l.spore_bytes_free(b)
    return out
'''


def _py_argtypes(fn):
    at = []
    for a in fn["args"]:
        t = a["t"]
        if t == "bytes":
            at += ["ctypes.c_char_p", "ctypes.c_size_t"]
        elif t in IN_SIZE:
            at += ["ctypes.c_char_p"]
        elif t in OUT_SIZE:
            at += ["ctypes.POINTER(ctypes.c_ubyte)"]
        elif t == "u32":
            at += ["ctypes.c_uint32"]
    return at


def gen_py():
    body = [PY_PRELUDE]
    for fn in FNS:
        c = "spore_" + fn["name"]
        at = _py_argtypes(fn)
        rt = {"void": "None", "bytes": "SporeBytes", "bool": "ctypes.c_ubyte"}[fn["ret"]]
        body.append(f"_l.{c}.argtypes = [{', '.join(at)}]")
        body.append(f"_l.{c}.restype = {rt}")

        params = [a["n"] for a in inputs(fn)]
        lines = [f"def {fn['name']}({', '.join(params)}):"]
        if fn.get("doc"):
            lines.append(f'    """{fn["doc"]}"""')
        callargs = []
        for a in fn["args"]:
            t, n = a["t"], a["n"]
            if t == "bytes":
                callargs += [n, f"len({n})"]
            elif t in IN_SIZE:
                callargs.append(n)
            elif t in OUT_SIZE:
                sz = OUT_SIZE[t]
                lines.append(f"    {n} = (ctypes.c_ubyte * {sz})()")
                callargs.append(n)
            elif t == "u32":
                callargs.append(n)
        lines.append(f"    _r = _l.{c}({', '.join(callargs)})")

        os_ = outs(fn)
        out_exprs = [f"bytes({o['n']})" for o in os_]
        if fn["ret"] == "bytes":
            lines.append("    return _take(_r)")
        elif fn["ret"] == "bool":
            if os_:
                joined = ", ".join(out_exprs)
                lines.append(f"    return ({joined}) if _r else None")
            else:
                lines.append("    return bool(_r)")
        else:  # void
            if len(out_exprs) == 1:
                lines.append(f"    return {out_exprs[0]}")
            else:
                lines.append(f"    return ({', '.join(out_exprs)})")
        body.append("\n".join(lines))
    return "\n\n".join(body) + "\n"


# -------------------------------------------------------------------------- Go
GO_PRELUDE = '''\
// Auto-generated SPORE bindings (cgo) — do not edit; run bindings/generate.py.
//
// Build/run needs libspore on the linker + loader path, e.g.:
//   LD_LIBRARY_PATH=../../target/release go test ./...
package spore

/*
#cgo CFLAGS: -I${SRCDIR}/..
#cgo LDFLAGS: -L${SRCDIR}/../../target/release -lspore
#include <stdlib.h>
#include "spore.h"
*/
import "C"
import "unsafe"

func take(b C.SporeBytes) []byte {
\tif b.data == nil {
\t\treturn nil
\t}
\tout := C.GoBytes(unsafe.Pointer(b.data), C.int(b.len))
\tC.spore_bytes_free(b)
\treturn out
}

func ptr(b []byte) *C.uint8_t {
\tif len(b) == 0 {
\t\treturn nil
\t}
\treturn (*C.uint8_t)(unsafe.Pointer(&b[0]))
}
'''


def gen_go():
    body = [GO_PRELUDE]
    for fn in FNS:
        c = "spore_" + fn["name"]
        params = []
        for a in inputs(fn):
            if a["t"] == "u32":
                params.append(f"{a['n']} uint32")
            else:
                params.append(f"{a['n']} []byte")
        os_ = outs(fn)
        # return type
        if fn["ret"] == "bytes":
            ret = "[]byte"
        elif fn["ret"] == "bool":
            ret = "[]byte" if os_ else "bool"
        else:  # void -> one []byte per output buffer
            ret = ", ".join(["[]byte"] * len(os_))
            if len(os_) != 1:
                ret = "(" + ret + ")"
        lines = []
        if fn.get("doc"):
            lines.append(f"// {camel(fn['name'])} — {fn['doc']}")
        lines.append(f"func {camel(fn['name'])}({', '.join(params)}) {ret} {{")
        callargs = []
        for a in fn["args"]:
            t, n = a["t"], a["n"]
            if t == "bytes":
                callargs += [f"ptr({n})", f"C.size_t(len({n}))"]
            elif t in IN_SIZE:
                callargs.append(f"ptr({n})")
            elif t in OUT_SIZE:
                sz = OUT_SIZE[t]
                lines.append(f"\tvar {n} [{sz}]byte")
                callargs.append(f"(*C.uint8_t)(unsafe.Pointer(&{n}[0]))")
            elif t == "u32":
                callargs.append(f"C.uint32_t({n})")
        call = f"C.{c}({', '.join(callargs)})"
        out_exprs = [f"{o['n']}[:]" for o in os_]
        if fn["ret"] == "bytes":
            lines.append(f"\treturn take({call})")
        elif fn["ret"] == "bool":
            if os_:
                lines.append(f"\tok := {call} != 0")
                lines.append("\tif !ok {")
                lines.append("\t\treturn nil")
                lines.append("\t}")
                lines.append(f"\treturn append([]byte(nil), {out_exprs[0]}...)")
            else:
                lines.append(f"\treturn {call} != 0")
        else:  # void
            lines.append(f"\t{call}")
            copies = [f"append([]byte(nil), {e}...)" for e in out_exprs]
            lines.append(f"\treturn {', '.join(copies)}")
        lines.append("}")
        body.append("\n".join(lines))
    return "\n\n".join(body) + "\n"


# -------------------------------------------------------------------------- JS
JS_PRELUDE = '''\
// Auto-generated SPORE bindings (koffi) — do not edit; run bindings/generate.py.
//
//   npm install koffi
//   SPORE_LIB=../../target/release/libspore.so node your.js
//
// Every function takes/returns Node Buffers; failing functions return null.
'use strict';
const koffi = require('koffi');
const path = require('path');

function loadLib() {
  const names = ['libspore.so', 'libspore.dylib', 'spore.dll'];
  const tried = [];
  if (process.env.SPORE_LIB) tried.push(process.env.SPORE_LIB);
  for (const n of names) {
    tried.push(path.join(__dirname, '..', '..', 'target', 'release', n));
    tried.push(n);
  }
  let last;
  for (const p of tried) {
    try { return koffi.load(p); } catch (e) { last = e; }
  }
  throw last;
}
const _l = loadLib();

const SporeBytes = koffi.struct('SporeBytes', { data: 'uint8_t *', len: 'size_t' });
const _free = _l.func('void spore_bytes_free(SporeBytes b)');

function take(b) {
  if (!b.data) return null;
  const out = Buffer.from(koffi.decode(b.data, koffi.array('uint8_t', b.len, 'Array')));
  _free(b);
  return out;
}
'''


def _js_proto(fn):
    ret = {"void": "void", "bytes": "SporeBytes", "bool": "uint8_t"}[fn["ret"]]
    params = []
    for a in fn["args"]:
        t, n = a["t"], a["n"]
        if t == "bytes":
            params += [f"uint8_t *{n}", f"size_t {n}_len"]
        elif t in IN_SIZE:
            params.append(f"uint8_t *{n}")
        elif t in OUT_SIZE:
            params.append(f"_Out_ uint8_t *{n}")
        elif t == "u32":
            params.append(f"uint32_t {n}")
    return f"{ret} spore_{fn['name']}({', '.join(params)})"


def gen_js():
    body = [JS_PRELUDE]
    exports = []
    for fn in FNS:
        c = "spore_" + fn["name"]
        body.append(f"const _{fn['name']} = _l.func('{_js_proto(fn)}');")
        params = [a["n"] for a in inputs(fn)]
        lines = [f"function {fn['name']}({', '.join(params)}) {{"]
        callargs = []
        for a in fn["args"]:
            t, n = a["t"], a["n"]
            if t == "bytes":
                callargs += [n, f"{n}.length"]
            elif t in IN_SIZE:
                callargs.append(n)
            elif t in OUT_SIZE:
                sz = OUT_SIZE[t]
                lines.append(f"  const {n} = Buffer.alloc({sz});")
                callargs.append(n)
            elif t == "u32":
                callargs.append(n)
        call = f"_{fn['name']}({', '.join(callargs)})"
        os_ = outs(fn)
        out_names = [o["n"] for o in os_]
        if fn["ret"] == "bytes":
            lines.append(f"  return take({call});")
        elif fn["ret"] == "bool":
            if os_:
                lines.append(f"  const ok = {call} !== 0;")
                lines.append(f"  return ok ? {out_names[0]} : null;")
            else:
                lines.append(f"  return {call} !== 0;")
        else:  # void
            lines.append(f"  {call};")
            if len(out_names) == 1:
                lines.append(f"  return {out_names[0]};")
            else:
                lines.append(f"  return [{', '.join(out_names)}];")
        lines.append("}")
        body.append("\n".join(lines))
        exports.append(fn["name"])
    body.append("module.exports = { " + ", ".join(exports) + " };")
    return "\n\n".join(body) + "\n"


def write(rel, text):
    path = os.path.join(HERE, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(text)
    print("wrote", os.path.relpath(path, HERE))


if __name__ == "__main__":
    write("spore.h", gen_c())
    write("python/spore.py", gen_py())
    write("go/spore.go", gen_go())
    write("node/spore.js", gen_js())
    print(f"generated {len(FNS)} functions for 4 targets")
