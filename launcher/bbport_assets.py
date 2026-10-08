"""Availability of the FSR 4.1.1 model required by a selected output/preset."""
from pathlib import Path


def fsr411_problem(directory: Path, output: str, preset: int) -> str | None:
    w, h = map(int, output.split("x"))
    model = ("t2160" if w > 1920 or h > 1080 else "t1080") + ("_m1" if preset == 4 else "_m0")
    passes = ["spd", "prepass", "pass0_post"]
    for i in range(1, 13):
        passes.extend([f"pass{i}", f"pass{i}_post"])
    passes.extend(["postpass", "rcas"])
    for name in passes:
        path = directory / model / (name + ".spv")
        if not path.is_file() or path.stat().st_size < 20 or path.stat().st_size % 4:
            return f"Missing or damaged file {model}/{name}.spv"
    path = directory / model / "initializer.bin"
    if not path.is_file() or path.stat().st_size != 131072:
        return f"Missing or damaged file {model}/initializer.bin"
    return None
