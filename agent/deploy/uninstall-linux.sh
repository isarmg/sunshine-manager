#!/usr/bin/env bash
set -euo pipefail
if [[ $(id -u) != 0 || $# != 0 ]]; then echo "Run as root without arguments." >&2; exit 1; fi
if [[ -L /etc/systemd/system/sunshine-agent.service || -L /opt/sunshine-agent ]]; then
  echo "Unexpected installation links; refusing removal." >&2; exit 1
fi
systemctl disable --now sunshine-agent.service
rm -- /etc/systemd/system/sunshine-agent.service /opt/sunshine-agent/sunshine-agent
rmdir -- /opt/sunshine-agent
systemctl daemon-reload
echo "Agent executable and service removed. State and service account retained for recovery/deduplication."
echo "Revoke the device in Manager. Do not purge /var/lib/sunshine-agent while operations remain unresolved."
