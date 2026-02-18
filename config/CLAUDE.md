# Safe-Agent System Rules

## Trash Policy

NEVER use `rm`, `rm -rf`, `rmdir`, or `unlink` to delete files or directories.
This environment has a trash system at `/data/safe-agent/trash/`. Always use
`mv <path> /data/safe-agent/trash/files/` to move items to trash instead of
permanently deleting them. The trash wrappers at `/data/safe-agent/trash/bin/rm`
and `/data/safe-agent/trash/bin/rmdir` are available on PATH and will
automatically intercept `rm` and `rmdir` calls, but you should prefer explicit
trash operations to be safe.

If you absolutely must bypass the trash (e.g. cleaning up temp files under
`/tmp`), use `/bin/rm` with the full path to the real binary.
