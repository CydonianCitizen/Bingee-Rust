# Shared benchmark posters

Synthetic poster art made for the Bingee Desktop spikes. It is not derived from
any real movie or TV poster. Both the Rust + Slint and the C# + Avalonia
branches use these files unchanged (`BENCHMARK_SPEC.md`, "Same poster
assets").

| | |
| --- | --- |
| Files | `poster-001.jpg` … `poster-100.jpg` (100 files, about 2.1 MB) |
| Format | Baseline JPEG, 8-bit RGB (no alpha), quality 85, 20.5–23.5 KB each |
| Size | 240 × 360 px (2:3) |
| Checksums | `SHA256SUMS` (`sha256sum -c SHA256SUMS`) |

## Mapping from media records

Poster number = `(local_id − 1) % 100 + 1`, so id 1 → `poster-001.jpg`,
id 100 → `poster-100.jpg`, id 101 → `poster-001.jpg`, and id 237 →
`poster-037.jpg`. The number is printed on the poster, so a wrong image is
visible at a glance. Every one of the 1,000 records maps to exactly one poster,
and each poster is used by 10 records.

A pool of 100 is larger than any sensible decoded-image cache for thumbnails
(the Rust spike's cache holds 36), so a long scroll forces eviction. It is also
small enough not to bloat the repository.

## Recreating them

```bash
cargo run --example generate_posters   # from the repo root
sha256sum -c benchmark/assets/posters/SHA256SUMS
```

`examples/generate_posters.rs` uses integer arithmetic only, with no RNG and no
fonts: a gradient sky, a disc, a striped ground, the number drawn with a 5×7
bitmap font, and a deterministic grain, so the JPEG carries photo-like
entropy. Neighbouring numbers get hues 137.5° apart. Re-running it has been
checked to produce byte-identical files with `image` 0.25.10's JPEG encoder. A
different encoder version may change the bytes: the committed files, not the
generator, are the reference.
