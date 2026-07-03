"""Media helpers: image conversion and duration formatting."""

from pathlib import Path
from PIL import Image
import io

Image.MAX_IMAGE_PIXELS = 25_000_000


def to_jpeg_bytes(path: Path, min_width: int | None = None,
                  max_width: int | None = None) -> bytes:
    """Read an image file and return JPEG bytes.

    If min_width is set and the image is narrower, upscale proportionally.
    If max_width is set and the image is wider, downscale proportionally.
    """
    with Image.open(path) as img:
        rgb = img.convert("RGB")
        if min_width and rgb.width < min_width:
            scale = min_width / rgb.width
            rgb = rgb.resize((min_width, int(rgb.height * scale)), Image.Resampling.LANCZOS)
        elif max_width and rgb.width > max_width:
            scale = max_width / rgb.width
            rgb = rgb.resize((max_width, int(rgb.height * scale)), Image.Resampling.LANCZOS)
        buf = io.BytesIO()
        rgb.save(buf, format="JPEG", quality=85)
        return buf.getvalue()


def humanize_duration(seconds: float | None) -> str:
    """Format seconds as h:mm:ss or m:ss."""
    if not seconds:
        return ""
    s = int(seconds)
    h, s = divmod(s, 3600)
    m, s = divmod(s, 60)
    if h:
        return f"{h}:{m:02d}:{s:02d}"
    return f"{m}:{s:02d}"
