#!/bin/sh

set -eu

data_dir=/data
host_key=$data_dir/ssh/ssh_host_ed25519_key
admin_key=/bootstrap/admin.pub

install -d -m 0700 -o gitbox -g gitbox "$data_dir"
install -d -m 0700 "$data_dir/ssh" /run/sshd

if [ ! -f "$data_dir/config.toml" ]; then
  if [ ! -s "$admin_key" ]; then
    echo "missing admin public key at $admin_key" >&2
    exit 1
  fi
  su-exec gitbox:gitbox /usr/local/bin/gitboxd init \
    --data-dir "$data_dir" \
    --admin "$GITBOX_ADMIN" \
    --ssh-key "$admin_key" \
    --executable /usr/local/bin/gitboxd
fi

if [ ! -s "$data_dir/.ssh/authorized_keys" ]; then
  echo "Gitbox authorized_keys is missing or empty" >&2
  exit 1
fi

if [ ! -f "$host_key" ]; then
  ssh-keygen -q -t ed25519 -N '' -f "$host_key"
fi
chown root:root "$host_key" "$host_key.pub"
chmod 0600 "$host_key"
chmod 0644 "$host_key.pub"

/usr/sbin/sshd -t
exec /usr/sbin/sshd -D -e
