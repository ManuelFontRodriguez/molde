"""Shared helpers for the idiomatic ports' reference solutions (judge-side only)."""
import importlib.util
from pathlib import Path

AC = Path(__file__).resolve().parent.parent


def plants(port):
    spec = importlib.util.spec_from_file_location("bench_setup", AC / "runner/setup.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.PLANTS[port]


def undo(port, task, special=None):
    """Inverse-plant solution: undo each plant, last first (they prove the clean state
    passes). `special` maps a plant index to the edits that undo it, for plants that
    create a file or delete text (no anchor to swap back)."""
    out = []
    for i, (rel, old, new, _cat) in reversed(list(enumerate(plants(port)[task]))):
        if special and i in special:
            out += special[i]
        else:
            out.append((rel, new, old))
    return out


def source(fw, rel):
    return (AC / "ports-idiomatic" / fw / "site" / rel).read_text()
