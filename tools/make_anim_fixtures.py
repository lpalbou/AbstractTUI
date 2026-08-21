#!/usr/bin/env python3
"""Regenerate the embedded animation fixtures in src/gfx/anim_fixtures.rs.

Writes a 3-frame 12x8 clip — a red bar stepping across a gradient — as
an animated GIF and an APNG (the two formats the engine decodes), plus
a structurally valid H.264 `.mp4` so the "refuse by name" path has a
real subject.

    pip install pillow        # the GIF/APNG encoder; the muxers are here
    python3 tools/make_anim_fixtures.py OUTDIR

Pillow writes the GIF and the APNG. The `.mp4` is muxed here rather
than by ffmpeg on purpose: the fixtures must not depend on which
optional tool a contributor happens to have installed.
"""
import struct
import sys
from PIL import Image

W, H, N, FPS = 12, 8, 3, 10
TIMESCALE = 600
DELTA = TIMESCALE // FPS


def frame_image(i):
    im = Image.new("RGB", (W, H), (20, 20, 20))
    px = im.load()
    for y in range(H):
        for x in range(W):
            # A TWO pixel bar, never touching the canvas edge: a
            # 1-pixel feature at the border smears under JPEG and makes
            # the fixture test the encoder instead of the demuxer.
            bar = x in (1 + i * 3, 2 + i * 3)
            px[x, y] = (240, 30, 30) if bar else (20 + x * 8, 20 + y * 8, 120)
    return im


# ------------------------------------------------------------- ISO base media
def box(btype, payload):
    return struct.pack(">I", len(payload) + 8) + btype + payload


def make_isobmff(fourcc, frames):
    ftyp = box(b"ftyp", b"qt  " + struct.pack(">I", 512) + b"qt  ")
    first_sample = len(ftyp) + 8  # right after the mdat header
    mdat = box(b"mdat", b"".join(frames))
    n = len(frames)
    unity = struct.pack(">9i", 0x10000, 0, 0, 0, 0x10000, 0, 0, 0, 0x40000000)
    mvhd = box(
        b"mvhd",
        struct.pack(">IIIIIIHHI", 0, 0, 0, TIMESCALE, DELTA * n, 0x10000, 0x100, 0, 0)
        + b"\x00" * 8 + unity + b"\x00" * 24 + struct.pack(">I", 2),
    )
    tkhd = box(
        b"tkhd",
        struct.pack(">IIIIIII", 0xF, 0, 0, 1, 0, DELTA * n, 0)
        + b"\x00" * 8 + unity + struct.pack(">II", W << 16, H << 16),
    )
    mdhd = box(b"mdhd", struct.pack(">IIIIIHH", 0, 0, 0, TIMESCALE, DELTA * n, 0x55C4, 0))
    hdlr = box(b"hdlr", struct.pack(">I", 0) + b"mhlr" + b"vide" + b"\x00" * 12 + bytes([0]))
    vmhd = box(b"vmhd", struct.pack(">IHHHH", 1, 0, 0, 0, 0))
    dinf = box(b"dinf", box(b"dref", struct.pack(">II", 0, 1) + box(b"alis", struct.pack(">I", 1))))
    entry = (
        struct.pack(">I", 86) + fourcc + b"\x00" * 6 + struct.pack(">H", 1)
        + struct.pack(">HHII", 0, 0, 0, 0) + b"\x00" * 8
        + struct.pack(">HHIIIHH", W, H, 0x00480000, 0x00480000, 0, 1, 0)
        + b"\x00" * 30 + struct.pack(">Hh", 24, -1)
    )
    stbl = box(
        b"stbl",
        box(b"stsd", struct.pack(">II", 0, 1) + entry)
        + box(b"stts", struct.pack(">IIII", 0, 1, n, DELTA))
        + box(b"stsc", struct.pack(">IIIII", 0, 1, 1, n, 1))
        + box(b"stsz", struct.pack(">III", 0, 0, n)
              + b"".join(struct.pack(">I", len(f)) for f in frames))
        + box(b"stco", struct.pack(">III", 0, 1, first_sample)),
    )
    moov = box(
        b"moov",
        mvhd + box(b"trak", tkhd + box(b"mdia", mdhd + hdlr + box(b"minf", vmhd + dinf + stbl))),
    )
    return ftyp + mdat + moov


def main(outdir="."):
    images = [frame_image(i) for i in range(N)]
    images[0].save(f"{outdir}/fx.gif", save_all=True, append_images=images[1:],
                   duration=100, loop=0)
    images[0].save(f"{outdir}/fx_apng.png", save_all=True, append_images=images[1:],
                   duration=100, loop=0)
    # Filler samples: nothing decodes this file, it only gets refused.
    open(f"{outdir}/fx_h264.mp4", "wb").write(make_isobmff(b"avc1", [b"\x00" * 16]))
    print(f"wrote fx.gif fx_apng.png fx_h264.mp4 into {outdir}")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else ".")
