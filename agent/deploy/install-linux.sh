#!/usr/bin/env bash
set -euo pipefail
# Explicit local installation only. Never invoked from a Manager task.
if [[ $(id -u) != 0 || $(uname -m) != x86_64 || $# != 2 ]]; then
  echo "Usage (root, Linux x86_64): install-linux.sh ABSOLUTE_AGENT_BINARY ABSOLUTE_PROTECTED_BOOTSTRAP" >&2
  exit 1
fi
binary=$1
bootstrap=$2
if [[ $binary != /* || $bootstrap != /* || ! -f $binary || -L $binary ]]; then exit 1; fi
for target in /opt/sunshine-agent /var/lib/sunshine-agent /etc/systemd/system/sunshine-agent.service; do
  if [[ -e $target || -L $target ]]; then
    echo "Refusing to overwrite an existing installation or state. Upgrade/restore belongs to sarmg-upgrade." >&2
    exit 1
  fi
done
if getent passwd sunshine-agent >/dev/null; then
  echo "Service account already exists; review it before installing." >&2
  exit 1
fi
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
install -d -m 0755 /opt/sunshine-agent
install -m 0755 -- "$binary" /opt/sunshine-agent/sunshine-agent
/opt/sunshine-agent/sunshine-agent init --state /var/lib/sunshine-agent --bootstrap "$bootstrap"
useradd --system --user-group --home-dir /var/lib/sunshine-agent --shell /usr/sbin/nologin sunshine-agent
# This directory was created above and was explicitly required not to exist.
chown -R sunshine-agent:sunshine-agent /var/lib/sunshine-agent
install -m 0644 "$script_dir/sunshine-agent.service" /etc/systemd/system/sunshine-agent.service
systemctl daemon-reload
systemctl enable --now sunshine-agent.service
systemctl is-active --quiet sunshine-agent.service
echo "Agent installed. Verify registration in Manager, then securely remove the original bootstrap."
