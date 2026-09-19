# Test fixtures

`gradient.heic` — a 640×480 10-bit HEIC of a generated colour gradient,
produced with `heif-enc`. Synthetic: it contains no photograph and nothing
derived from anyone's backup.

It exists so the thumbnail pipeline is exercised against real HEVC data rather
than only against mocks. Regenerate with:

    convert -size 640x480 gradient:'#0a84ff-#ff9f0a' -swirl 45 fx.png
    heif-enc -q 78 fx.png -o gradient.heic

Known gap: this file is a single HEVC tile. iPhone photos above a few megapixels
are stored as a *grid* of tiles, and that path is not covered by any fixture
here, because the libheif build used to generate this one cannot produce tiled
output. `heif-oxide` implements grid decoding (ISO 23008-12 §6.6.2.3.2) and has
its own tests for it; the first real check is a genuine camera roll.
