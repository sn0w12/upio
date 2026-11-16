# Preprocessing

Files can be transformed before upload so they fit the service's limits. Rules
match files by MIME type and **chain**: they always run in a fixed canonical
order (`normalize_name` → `compress_image` → `wrap_zip` → `split_video`),
feeding each output into the next rule regardless of how they are listed.
Preprocessing is configured globally in the config file and can be replaced per
service:

```toml
[preprocess]
enabled = true

[[preprocess.rules]]
strategy = "normalize_name"
params = { replacement = "_", lowercase = true }

[[preprocess.rules]]
strategy = "compress_image"
params = { format = "jpeg", quality = 80, max_dimensions = "4096x4096" }

# Optional per-service replacement. This table is used instead of the global
# one for bunkr; nothing is merged with the global rules.
[bunkr.preprocess]
enabled = true

[[bunkr.preprocess.rules]]
strategy = "split_video"
```

Services without a `[service.preprocess]` table inherit the global
configuration. An override that only sets `enabled = false` disables
preprocessing for that service without touching the other services.

## Rule fields

A rule has exactly two fields; being listed is what enables it:

| Field      | Description                                  |
| ---------- | -------------------------------------------- |
| `strategy` | The transformation to apply.                 |
| `params`   | Strategy-specific options (all defaultable). |

The strategy owns everything else:

- Which MIME types it applies to. Non-matching files pass through untouched.
- When it runs: on every match, or only when the file exceeds the target
  service's maximum file size. The threshold always comes from the service,
  never from the config.
- Which external tools it needs. A missing tool skips the rule instead of
  failing the upload.

There is no per-rule `trigger`, `match_types`, or other override field. Such
keys in the config are ignored.

Temporary artifacts are removed automatically after upload. If cleanup fails,
the upload's URLs are preserved but the result reports the cleanup error.

## Strategies

Strategies run in this canonical chain order:

<!-- BEGIN generated: strategies -->

| #   | Strategy         | Trigger      | What it does                                                                                                                    | Requirements               |
| --- | ---------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------- | -------------------------- |
| 1   | `normalize_name` | Always       | Sanitize the file name (spaces, unicode, invalid characters); length-capped, Unicode-safe. The original file is never modified. | None (pure Rust)           |
| 2   | `compress_image` | If oversized | Re-encode and/or resize images; outputs jpeg or png.                                                                            | None (pure Rust)           |
| 3   | `wrap_zip`       | Always       | Wrap the file in a compressed zip (useful for text and log files). Skips images and videos.                                     | None (pure Rust)           |
| 4   | `split_video`    | If oversized | Split a video into independently playable segments via ffmpeg and ffprobe.                                                      | ffmpeg + ffprobe on `PATH` |

<!-- END generated: strategies -->

When a strategy needs an external tool that is missing, the rule is skipped
rather than failing the upload. Use `upio preprocess check` to see what is
available.
