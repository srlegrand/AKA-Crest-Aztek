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


# Run before each command: lets aztek.api write out point edits it holds back.
_before_call = []
_plain_arg_commands = set()


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
        if _before_call:
            _before_call[0]()
        if name in _plain_arg_commands:
            # A plug-in command with no syntax takes its flags as plain arguments.
            for k, v in kwargs.items():
                flat.append("-" + k)
                flat.extend(v if isinstance(v, (list, tuple)) else [v])
            kwargs = {}
        out = _aztek.call(name, json.dumps(_plain(flat)), json.dumps(_plain(kwargs)))
        result = json.loads(out)
        if isinstance(result, dict) and "__error__" in result:
            raise RuntimeError(result["__error__"])
        # Maya returns compound attributes as a list of one tuple; a multi attribute of as many elements stays a list.
        if name == "getAttr" and isinstance(result, list) and len(result) in (2, 3, 4) and all(isinstance(x, (int, float)) for x in result) and not (kwargs or {}).get("mi", (kwargs or {}).get("multiIndices")):
            size = json.loads(_aztek.call("getAttr", json.dumps(_plain(flat)), json.dumps({"size": True})))
            if size == 1:
                return [tuple(result)]
        # Maya's list queries give None rather than an empty list.
        if name in ("listRelatives", "listConnections", "listAttr") and result == []:
            return None
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


class _ApiImporter:
    """aztek.api.OpenMaya and OpenMayaAnim, embedded in the host and loaded on first import; maya.api.* are the same."""

    PARTS = {
        "OpenMaya": ["om_values", "om_core", "om_fn", "om_iter", "om_plugin"],
        "OpenMayaAnim": ["om_anim"],
    }

    def find_spec(self, name, path=None, target=None):
        parts = name.split(".")
        if len(parts) >= 2 and parts[0] in ("aztek", "maya") and parts[1] == "api":
            import importlib.machinery as m

            if len(parts) == 2:
                return m.ModuleSpec(name, self, is_package=True)
            if len(parts) == 3 and parts[2] in self.PARTS:
                return m.ModuleSpec(name, self)
        return None

    def create_module(self, spec):
        parts = spec.name.split(".")
        if len(parts) == 3:
            # One module under both names: the other may have been imported first.
            other = sys.modules.get(("maya" if parts[0] == "aztek" else "aztek") + ".api." + parts[2])
            if other is not None:
                return other
        return None

    def exec_module(self, module):
        parts = module.__spec__.name.split(".")
        if len(parts) != 3 or "_loaded" in module.__dict__:
            return
        module.__dict__["_loaded"] = True
        for part in self.PARTS[parts[2]]:
            exec(compile(_aztek.source(part), "<%s>" % part, "exec"), module.__dict__)


sys.meta_path.insert(0, _ApiImporter())


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
        if _before_call:
            try:
                _before_call[0]()
            except Exception as e:
                _aztek.write("# Error: %s #" % (e,), 2)
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
