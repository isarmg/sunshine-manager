# Sunshine Manager 0.7 release

This archive contains one immutable Linux x86_64 release. Extract its `0.7.0/` directory directly
under `/opt/isarmg/sunshine-manager/releases/`. Do not merge its files into another release tree.

```bash
tar -xzf sunshine-manager-0.7.0-x86_64-unknown-linux-gnu.tar.gz \
  -C /opt/isarmg/sunshine-manager/releases
```

Before activation, run the contained verifier from the physical version directory:

```bash
/opt/isarmg/sunshine-manager/releases/0.7.0/bin/sunshine-manager \
  verify-release --root /opt/isarmg/sunshine-manager/releases/0.7.0
```

Install the included systemd unit and configure `/etc/isarmg/sunshine-manager.env`. Its paths are
fixed to `releases/0.7.0`; the same Rust process verifies the complete file manifest, confirms that
the configured Web directory belongs to that release, and only then opens state or starts serving.

This release contains no migration, backup or restore path. Use the independent `isarmg-upgrade`
repository to back up state, transform an older version, verify recovery and activate a new release.
