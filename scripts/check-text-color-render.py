"""Actual FFmpeg foreground command/alpha probe; requires drawtext-enabled FFmpeg."""
import argparse
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument("--ffmpeg", required=True)
parser.add_argument("--font", required=True)
args = parser.parse_args()
# Deliberately use a fractional frame clock whose nearest microsecond rounds up.
colors = [(255, 0, 0, 255), (0, 0, 255, 128), (0, 255, 0, 0)]
with tempfile.TemporaryDirectory(prefix="video-kadr-color-") as directory:
    commands = Path(directory) / "commands"
    commands.write_text("".join(
        f"{(frame * 1001000000 // 30000) / 1000000:.6f} "
        f"drawtext@text_1 fontcolor 0x{r:02X}{g:02X}{b:02X}@1.000000000, "
        f"colorchannelmixer@text_1 aa {a / 255:.9f};\n"
        for frame, (r, g, b, a) in enumerate(colors)
    ))
    def escape(value):
        return "'" + str(value).replace("\\", "\\\\").replace("'", "'\\''").replace(":", "\\:") + "'"
    graph = (
        "color=black@0.0:s=64x64:r=30000/1001,format=rgba,"
        f"sendcmd=f={escape(commands)},drawtext@text_1=fontfile={escape(args.font)}:"
        "text=M:fontsize=48:x=0:y=0:fontcolor=white,colorchannelmixer@text_1=aa=1"
    )
    rendered = subprocess.run([
        args.ffmpeg, "-v", "error", "-f", "lavfi", "-i", graph,
        "-frames:v", "3", "-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1",
    ], capture_output=True, check=True).stdout
    frame_size = 64 * 64 * 4
    assert len(rendered) == frame_size * 3
    frames = [rendered[i * frame_size:(i + 1) * frame_size] for i in range(3)]
    interior = max(range(0, frame_size, 4), key=lambda i: frames[0][i + 3])
    actual = [tuple(frame[interior:interior + 4]) for frame in frames]
    assert actual == colors, (actual, colors)
    # Coverage remains proportional at antialiased glyph edges as well.
    for i in range(0, frame_size, 4):
        assert abs(frames[1][i + 3] - round(frames[0][i + 3] * 128 / 255)) <= 1
        assert frames[2][i + 3] == 0
    print("Actual text foreground fractional-FPS RGB/alpha/edge coverage PASS", actual)
