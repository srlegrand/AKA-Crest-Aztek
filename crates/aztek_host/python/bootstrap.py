# Sets up the aztek package (aztek.cmds, aztek.mel), the maya shim and the script editor's output.
import json
import sys
import types

import _aztek


class _Out:
    def __init__(self, kind):
        self.kind = kind
        self.buf = ""

    def write(self, text):
        self.buf += text
        while "\n" in self.buf:
            line, self.buf = self.buf.split("\n", 1)
            _aztek.write(line, self.kind)
        return len(text)

    def flush(self):
        if self.buf:
            _aztek.write(self.buf, self.kind)
            self.buf = ""


sys.stdout = _Out(3)
sys.stderr = _Out(2)


def _plain(v):
    if isinstance(v, (list, tuple)):
        return [_plain(x) for x in v]
    if isinstance(v, dict):
        return {k: _plain(x) for k, x in v.items()}
    return v


def _command(name):
    def run(*args, **kwargs):
        flat = []
        for a in args:
            # Maya takes lists of names wherever it takes names.
            if isinstance(a, (list, tuple)) and all(isinstance(x, str) for x in a):
                flat.extend(a)
            else:
                flat.append(a)
        out = _aztek.call(name, json.dumps(_plain(flat)), json.dumps(_plain(kwargs)))
        result = json.loads(out)
        if isinstance(result, dict) and "__error__" in result:
            raise RuntimeError(result["__error__"])
        # Maya returns compound attributes as a list of one tuple.
        if name == "getAttr" and isinstance(result, list) and len(result) == 3 and all(isinstance(x, (int, float)) for x in result):
            return [tuple(result)]
        return result

    run.__name__ = name
    run.__doc__ = _aztek.doc(name)
    return run


class _Cmds(types.ModuleType):
    """aztek.cmds: every Aztek command as a function. Flags are keyword arguments, long or short names."""

    def __getattr__(self, name):
        if name.startswith("__"):
            raise AttributeError(name)
        if not _aztek.exists(name):
            raise AttributeError("aztek.cmds has no command %r" % name)
        f = _command(name)
        setattr(self, name, f)
        return f

    def __dir__(self):
        return sorted(json.loads(_aztek.names()))


def _mel_eval(src):
    result = json.loads(_aztek.mel(src))
    if isinstance(result, dict) and "__error__" in result:
        raise RuntimeError(result["__error__"])
    return result


aztek = types.ModuleType("aztek")
aztek.__path__ = []
cmds = _Cmds("aztek.cmds")
mel = types.ModuleType("aztek.mel")
mel.eval = _mel_eval
aztek.cmds = cmds
aztek.mel = mel
sys.modules["aztek"] = aztek
sys.modules["aztek.cmds"] = cmds
sys.modules["aztek.mel"] = mel

# Scripts written for Maya import maya.cmds and maya.mel; both are Aztek's.
maya = types.ModuleType("maya")
maya.__path__ = []
maya.cmds = cmds
maya.mel = mel
sys.modules["maya"] = maya
sys.modules["maya.cmds"] = cmds
sys.modules["maya.mel"] = mel

import __main__

__main__.cmds = cmds


def _run(src):
    """Runs script editor source in __main__; a lone expression's value is shown as Maya shows it."""
    import traceback

    g = __main__.__dict__
    try:
        try:
            code = compile(src, "<script>", "eval")
        except SyntaxError:
            code = None
        if code is not None:
            value = eval(code, g)
            if value is not None:
                _aztek.write("# Result: %r #" % (value,), 1)
        else:
            exec(compile(src, "<script>", "exec"), g)
    except Exception as e:
        lines = traceback.format_exception(type(e), e, e.__traceback__)
        for l in "".join(lines[1:-1]).rstrip().splitlines():
            _aztek.write("# " + l, 2)
        _aztek.write("# Error: %s: %s #" % (type(e).__name__, e), 2)
    finally:
        sys.stdout.flush()
        sys.stderr.flush()


def _eval_from_mel(src):
    """MEL's python(): a lone expression's value as JSON, statements give null. Exceptions go back to MEL."""
    g = __main__.__dict__
    try:
        try:
            code = compile(src, "<python>", "eval")
        except SyntaxError:
            exec(compile(src, "<python>", "exec"), g)
            return "null"
        return json.dumps(_plain(eval(code, g)), default=str)
    finally:
        sys.stdout.flush()
        sys.stderr.flush()


__main__.__dict__["_aztek_run"] = _run
__main__.__dict__["_aztek_eval"] = _eval_from_mel
